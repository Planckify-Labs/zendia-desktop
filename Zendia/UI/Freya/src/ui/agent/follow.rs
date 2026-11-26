/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! How the agent panel's conversation scrolls (`FS-01`, `FS-02`, `FS-03`, `AP-15`).
//!
//! Uses [`zendia_browser_core::agent::follow_step`] and [`zendia_browser_core::agent::runway_height`] as the single
//! source of truth for spring physics and held runway room, reserves a permanent `32 px` foot lane, detects
//! `landedWhole` jumps (`delta > view / 2`), snaps immediately on initial mount/conversation switch, and shows a
//! `150 ms`-debounced `"New reply"` / `"Jump to latest"` pill with `180 ms` / `120 ms` fade + slide animation.

use crate::ui::kit::{focus_ring_border, use_interactive_feedback};
use crate::ui::theme::{ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::{Duration, Instant};
use zendia_browser_core::agent::{FollowState, follow_step, runway_height};

/// How long a sent request takes to glide to the top.
pub const GLIDE: Duration = Duration::from_millis(380);
/// Within this distance of the end, the view follows the end again.
pub const STICK_DISTANCE: f32 = 70.0;
/// The jump button shows beyond this distance from the end...
pub const JUMP_SHOW_DISTANCE: f32 = 320.0;
/// ...and hides only within this one, so it does not flicker during a gesture.
pub const JUMP_HIDE_DISTANCE: f32 = 2.0;
/// Show debounce delay for the jump button (`150 ms`, matching `ZendiaAgentFollow.sys.mjs:27`).
pub const JUMP_DELAY: Duration = Duration::from_millis(150);
/// Permanent foot lane height at the bottom of the conversation (`32 px`, matching `zendia.css:3293`).
pub const FOOT_LANE: f32 = 32.0;
/// Space kept between the top of the view and a request that glided there.
pub const PROMPT_TOP_GAP: f32 = 12.0;

const FRAME_MS: f32 = 1000.0 / 60.0;
const FRAME: Duration = Duration::from_millis(16);
const MAX_FRAMES: f32 = 8.0;
const GROWTH_SMOOTHING: f32 = 0.12;

/// Ease-out curve for the glide: fast at first, settling at the end (`cubic-bezier(.5, 1, .89, 1)`).
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

/// Room to hold open below the content so a request at `prompt_top` can sit at the top of a `viewport` tall view,
/// keeping at least [`FOOT_LANE`] at rest.
pub fn held_room(viewport: f32, content: f32, prompt_top: f32) -> f32 {
    runway_height(viewport, (content - prompt_top).max(0.0), FOOT_LANE)
}

/// Whether the jump button wants to be shown, given whether it showed before and how far the view is from the end.
pub fn jump_visible(was_visible: bool, distance_from_end: f32) -> bool {
    if distance_from_end > JUMP_SHOW_DISTANCE {
        true
    } else if distance_from_end <= JUMP_HIDE_DISTANCE {
        false
    } else {
        was_visible
    }
}

/// View state of the conversation scroll, kept by the agent panel.
#[derive(Clone, Copy, PartialEq)]
pub struct ConversationFollow {
    controller: ScrollController,
    /// Whether the view follows the end. Scrolling away turns it off; scrolling back near the end turns it on.
    pub following: State<bool>,
    /// Room held open below the content (at least `FOOT_LANE = 32 px`).
    pub room: State<f32>,
    /// Offset of the last request from the top of the content, once measured.
    prompt_top: State<Option<f32>>,
    /// Content height below the prompt when last measured, for `landedWhole` detection (`FS-02`).
    held_content: State<f32>,
    /// Bumped to cancel a running glide or spring.
    generation: State<u64>,
    /// Whether the spring is running, so more growth keeps it going instead of starting it over.
    spring_running: State<bool>,
    /// Window y and height of the content, as last laid out.
    content: State<Option<(f32, f32)>>,
    /// Window y of the latest request, as last laid out.
    prompt_y: State<Option<f32>>,
    /// Whether a request was sent and waits to be measured before it glides to the top.
    glide_pending: State<bool>,
    /// Whether an initial mount or conversation switch is waiting for valid bounds to snap immediately to the end.
    snap_pending: State<bool>,
}

/// Creates the conversation scroll state for `controller`.
pub fn use_conversation_follow(controller: ScrollController) -> ConversationFollow {
    ConversationFollow {
        controller,
        following: use_state(|| true),
        room: use_state(|| FOOT_LANE),
        prompt_top: use_state(|| None),
        held_content: use_state(|| 0.0),
        generation: use_state(|| 0),
        spring_running: use_state(|| false),
        content: use_state(|| None),
        prompt_y: use_state(|| None),
        glide_pending: use_state(|| false),
        snap_pending: use_state(|| true),
    }
}

impl ConversationFollow {
    /// Current offset from the top and the largest offset the content allows.
    fn offset_and_end(&self) -> (f32, f32) {
        let offset = -self.controller.position().y;
        let end = self
            .controller
            .bounds
            .peek()
            .map_or(0.0, |(content, viewport)| (content.height - viewport.height).max(0.0));
        (offset, end)
    }

    fn viewport_height(&self) -> f32 {
        self.controller
            .bounds
            .peek()
            .map_or(0.0, |(_, viewport)| viewport.height)
    }

    fn next_generation(&mut self) -> u64 {
        let next = *self.generation.peek() + 1;
        self.generation.set(next);
        self.spring_running.set_if_modified(false);
        next
    }

    /// Starts a conversation fresh: permanent `32 px` foot lane, following the end, snapping immediately once laid out.
    pub fn reset(&mut self) {
        self.next_generation();
        self.glide_pending.set_if_modified(false);
        self.room.set_if_modified(FOOT_LANE);
        self.prompt_top.set_if_modified(None);
        self.held_content.set_if_modified(0.0);
        self.following.set_if_modified(true);
        self.snap_pending.set_if_modified(true);
        self.try_snap_to_end();
    }

    /// Snaps immediately to the bottom when `snap_pending` is set and viewport bounds are known (`AP-15`).
    pub fn try_snap_to_end(&mut self) -> bool {
        if !*self.snap_pending.peek() {
            return false;
        }
        let Some((content, viewport)) = *self.controller.bounds.peek() else {
            return false;
        };
        if viewport.height <= 0.0 || content.height <= 0.0 {
            return false;
        }
        let end = (content.height - viewport.height).max(0.0);
        if end > 0.5 {
            self.controller.scroll_to_y(-end.round() as i32);
        }
        self.snap_pending.set_if_modified(false);
        true
    }

    /// The user scrolled up: let go of the end and stop any motion.
    pub fn release(&mut self) {
        self.next_generation();
        self.prompt_top.set_if_modified(None);
        self.following.set_if_modified(false);
    }

    /// Re-attaches to the end when the user scrolled back within reach of it.
    pub fn restick_if_near_end(&mut self) {
        if *self.following.peek() {
            return;
        }
        let (offset, end) = self.offset_and_end();
        if end - offset <= STICK_DISTANCE {
            self.room.set_if_modified(FOOT_LANE);
            self.following.set(true);
        }
    }

    /// A request was sent. It glides to the top once both it and the content are laid out.
    pub fn request_glide(&mut self) {
        self.prompt_y.set(None);
        self.held_content.set(0.0);
        self.glide_pending.set(true);
    }

    /// The content was laid out at window y `y`, `height` tall with the held room.
    pub fn content_sized(&mut self, y: f32, height: f32) {
        self.content.set_if_modified(Some((y, height)));
        if *self.snap_pending.peek() {
            let _ = self.try_snap_to_end();
            return;
        }
        if !self.try_glide() {
            self.content_resized(height);
        }
    }

    /// The latest request was laid out at window y `y`.
    pub fn prompt_sized(&mut self, y: f32) {
        self.prompt_y.set_if_modified(Some(y));
        self.try_glide();
    }

    fn try_glide(&mut self) -> bool {
        if !*self.glide_pending.peek() {
            return false;
        }
        let (Some(prompt_y), Some((content_y, height))) = (*self.prompt_y.peek(), *self.content.peek()) else {
            return false;
        };
        self.glide_pending.set(false);
        let room = *self.room.peek();
        self.glide_prompt_to_top(prompt_y - content_y, height - room);
        true
    }

    /// Holds room below the newly sent prompt and glides it to the top.
    fn glide_prompt_to_top(&mut self, prompt_top: f32, content: f32) {
        let top = (prompt_top - PROMPT_TOP_GAP).max(0.0);
        self.prompt_top.set(Some(top));
        self.held_content.set((content - top).max(0.0));
        self.following.set(true);
        self.room.set(held_room(self.viewport_height(), content, top));
        self.glide_to(top);
    }

    /// Shrinks held room as the answer grows; detects `landedWhole` (`delta > view / 2`, `FS-02`).
    fn content_resized(&mut self, content_with_room: f32) {
        let room = *self.room.peek();
        let prompt_top_opt = *self.prompt_top.peek();
        if let Some(prompt_top) = prompt_top_opt {
            let view = self.viewport_height();
            let content_without_room = (content_with_room - room).max(0.0);
            let under_prompt = (content_without_room - prompt_top).max(0.0);
            let prev_held = *self.held_content.peek();
            let landed_whole = view > 0.0 && (under_prompt - prev_held) > (view * 0.5);
            self.held_content.set(under_prompt);

            let next = held_room(view, content_without_room, prompt_top);
            if (next - room).abs() > 0.5 {
                self.room.set(next);
            }
            if next <= FOOT_LANE {
                self.prompt_top.set(None);
                if landed_whole {
                    self.following.set(false);
                    return;
                }
            } else {
                return;
            }
        }
        if *self.following.peek() {
            self.follow_end();
        }
    }

    /// Smoothly springs to the end and follows it from there, for the jump button (`FS-03`).
    pub fn jump_to_end(&mut self) {
        self.prompt_top.set_if_modified(None);
        self.room.set_if_modified(FOOT_LANE);
        self.following.set(true);
        self.follow_end();
    }

    fn glide_to(&mut self, to: f32) {
        let generation = self.next_generation();
        let (from, _) = self.offset_and_end();
        let mut controller = self.controller;
        let generation_state = self.generation;
        let started = Instant::now();
        spawn(async move {
            loop {
                timer(FRAME).await;
                if *generation_state.peek() != generation {
                    break;
                }
                let t = started.elapsed().as_secs_f32() / GLIDE.as_secs_f32();
                let offset = from + (to - from) * ease_out(t);
                controller.scroll_to_y(-offset.round() as i32);
                if t >= 1.0 {
                    break;
                }
            }
        });
    }

    /// Runs the follow spring using [`zendia_browser_core::agent::follow_step`] with elapsed frame deltas (`FS-01`).
    fn follow_end(&mut self) {
        if *self.spring_running.peek() {
            return;
        }
        let generation = self.next_generation();
        let (offset, initial_end) = self.offset_and_end();
        if (initial_end - offset).abs() < 0.5 {
            return;
        }
        self.spring_running.set(true);
        let mut controller = self.controller;
        let generation_state = self.generation;
        let mut spring_running = self.spring_running;
        let this = *self;
        spawn(async move {
            let mut state = FollowState {
                position: offset,
                velocity: 0.0,
                growth: 0.0,
            };
            let mut last_target = initial_end;
            let mut last_tick = Instant::now();
            loop {
                timer(FRAME).await;
                if *generation_state.peek() != generation {
                    break;
                }
                let now = Instant::now();
                let dt_ms = now.duration_since(last_tick).as_secs_f32() * 1000.0;
                last_tick = now;
                let frames = (dt_ms / FRAME_MS).clamp(1.0, MAX_FRAMES);
                let (_, target) = this.offset_and_end();
                let grown = target - last_target;
                last_target = target;
                state.growth = if grown < -1.0 {
                    0.0
                } else {
                    state.growth + GROWTH_SMOOTHING * (grown.max(0.0) / frames - state.growth)
                };
                let mut left = frames;
                while left > 0.0 {
                    let step = left.min(1.0);
                    state = follow_step(state, target, step);
                    left -= 1.0;
                }
                if target - state.position < 0.5 && state.growth < 0.05 {
                    controller.scroll_to_y(-target.round() as i32);
                    spring_running.set(false);
                    break;
                }
                controller.scroll_to_y(-state.position.round() as i32);
            }
        });
    }
}

/// The `"New reply"` / `"Jump to latest"` pill (`FS-03`), shown `150 ms` after scrolling far from the end.
/// Lives in its own component so scrolling never redraws the conversation tree above it.
#[derive(Clone, PartialEq)]
pub struct JumpToLatest {
    follow: ConversationFollow,
    is_working: bool,
    key: DiffKey,
}

impl KeyExt for JumpToLatest {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl JumpToLatest {
    pub fn new(follow: ConversationFollow, is_working: bool) -> Self {
        Self {
            follow,
            is_working,
            key: DiffKey::None,
        }
    }
}

impl Component for JumpToLatest {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut shown = use_state(|| false);
        let mut pending_seq = use_state(|| 0u64);
        let mut mounted = use_state(|| false);
        let mut unmount_seq = use_state(|| 0u64);
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let mut follow = self.follow;

        let position = follow.controller.position();
        let bounds = *follow.controller.bounds.read();
        if bounds.is_some() && *follow.snap_pending.peek() {
            follow.try_snap_to_end();
        }
        let (end, view_w) = bounds.map_or((0.0, 0.0), |(content, viewport)| {
            ((content.height - viewport.height).max(0.0), viewport.width)
        });
        let distance = end + position.y;
        let free = !*follow.following.read();
        let want_show = free && jump_visible(*shown.peek(), distance);

        if want_show && !*shown.peek() {
            if *pending_seq.peek() == 0 {
                let seq = pending_seq.peek().wrapping_add(1).max(1);
                pending_seq.set(seq);
                spawn(async move {
                    timer(JUMP_DELAY).await;
                    if *pending_seq.peek() == seq {
                        pending_seq.set(0);
                        shown.set(true);
                        mounted.set(true);
                    }
                });
            }
        } else if !want_show {
            if *pending_seq.peek() != 0 {
                pending_seq.set(0);
            }
            if *shown.peek() {
                shown.set(false);
                let u_seq = unmount_seq.peek().wrapping_add(1);
                unmount_seq.set(u_seq);
                spawn(async move {
                    timer(Duration::from_millis(130)).await;
                    if *unmount_seq.peek() == u_seq && !*shown.peek() {
                        mounted.set(false);
                    }
                });
            }
        }
        follow.restick_if_near_end();

        let is_shown = *shown.read();
        let anim = use_animation_with_dependencies(&is_shown, |conf, active| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            if *active {
                AnimNum::new(0.0, 1.0)
                    .duration(Duration::from_millis(180))
                    .ease(Ease::Out)
                    .function(motion::ease_out_fn())
            } else {
                AnimNum::new(1.0, 0.0)
                    .duration(Duration::from_millis(120))
                    .ease(Ease::Out)
                    .function(motion::css_ease_out_fn())
            }
        });

        if !*mounted.read() {
            return rect().into_element();
        }

        let t = anim.get().value().clamp(0.0, 1.0);
        let label_text = if self.is_working { "New reply" } else { "Jump to latest" };
        let pill_w = if self.is_working { 88.0 } else { 112.0 };
        let left_x = ((view_w - pill_w) * 0.5).max(12.0);
        let bg = fb.background(
            theme.color(ThemeRole::Surface),
            theme.color(ThemeRole::SurfaceRaised),
            theme.color(ThemeRole::SurfaceRaised),
        );
        let base_border = Border::new()
            .width(0.5)
            .alignment(BorderAlignment::Inner)
            .fill(theme.glass.outline.to_color());
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, base_border);

        // FS-03: Positioned directly at its own pill box (no full-width click-blocking wrapper!),
        // layered above deep scrolled Markdown/table trees so content never bleeds through.
        rect()
            .layer(Layer::Relative(60))
            .position(Position::new_absolute().bottom(6.0).left(left_x))
            .offset_y((1.0 - t) * 8.0)
            .opacity(t)
            .a11y_id(a11y_id)
            .a11y_focusable(is_shown)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(label_text)
            .height(Size::px(30.0))
            .padding(Gaps::new(0.0, 12.0, 0.0, 12.0))
            .corner_radius(CornerRadius::new_all(16.0))
            .background(bg)
            .border(border)
            .shadow(Shadow::new().y(4.0).blur(12.0).color(Color::from_argb(46, 0, 0, 0)))
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                follow.jump_to_end();
            })
            .child(
                label()
                    .text(label_text)
                    .font_size(12.0)
                    .font_weight(FontWeight::MEDIUM)
                    .color(theme.color(ThemeRole::Text))
                    .max_lines(1),
            )
            .into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_lets_a_short_answer_sit_below_a_request_at_the_top() {
        // A 600 tall view, 1000 of content, and the request starts at 900: 500 + 32 lane.
        assert_eq!(held_room(600.0, 1000.0, 900.0), 532.0);
        // The answer grows by 300: the room shrinks by as much, keeping the 32px foot lane.
        assert_eq!(held_room(600.0, 1300.0, 900.0), 232.0);
        // Once the answer passes the bottom of the view, the permanent 32px foot lane remains.
        assert_eq!(held_room(600.0, 1600.0, 900.0), FOOT_LANE);
    }

    #[test]
    fn jump_button_shows_far_from_the_end_and_hides_only_at_it() {
        assert!(!jump_visible(false, 200.0));
        assert!(jump_visible(false, 400.0));
        assert!(jump_visible(true, 200.0));
        assert!(!jump_visible(true, 1.0));
    }

    #[test]
    fn glide_eases_out_and_ends_at_its_target() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        assert_eq!(ease_out(2.0), 1.0);
        assert!(ease_out(0.5) > 0.5);
    }

    #[test]
    fn core_follow_step_reaches_still_end_and_leads_growing_end() {
        let mut s = FollowState::default();
        for _ in 0..600 {
            s = follow_step(s, 400.0, 1.0);
            assert!(s.position <= 400.0);
        }
        assert!(400.0 - s.position < 0.5);

        let mut g = FollowState {
            position: 0.0,
            velocity: 0.0,
            growth: 2.0,
        };
        let mut end = 0.0;
        for _ in 0..240 {
            end += 2.0;
            g = follow_step(g, end, 1.0);
            assert!(g.position <= end);
        }
        assert!(end - g.position < 1.0, "expected growth lead to track growing end");

        let mut no_lead = FollowState::default();
        let mut end2 = 0.0;
        for _ in 0..240 {
            end2 += 2.0;
            no_lead = follow_step(no_lead, end2, 1.0);
            assert!(no_lead.position <= end2);
        }
        let behind = end2 - no_lead.position;
        assert!(behind > 4.0 && behind < 60.0, "{behind}");
    }
}
