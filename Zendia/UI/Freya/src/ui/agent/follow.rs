/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! How the agent panel's conversation scrolls.
//!
//! A sent request glides to the top of the view, and the room below it is held open, so the answer grows into that
//! room and the view does not move line by line. An answer longer than the view is followed by a spring that stays a
//! little above the end, so the newest line is not pressed against the edge. Scrolling away lets go of all of it, and
//! scrolling back near the end picks the end up again.
//!
//! Offsets here count down from the top of the content, so they are the negation of Freya's scroll position.

use freya::prelude::*;
use std::time::{Duration, Instant};

/// How long a sent request takes to glide to the top.
pub const GLIDE: Duration = Duration::from_millis(380);
/// Within this distance of the end, the view follows the end again.
pub const STICK_DISTANCE: f32 = 70.0;
/// The jump button shows beyond this distance from the end...
pub const JUMP_SHOW_DISTANCE: f32 = 320.0;
/// ...and hides only within this one, so it does not flicker during a gesture.
pub const JUMP_HIDE_DISTANCE: f32 = 2.0;
/// Space kept between the top of the view and a request that glided there.
pub const PROMPT_TOP_GAP: f32 = 12.0;

const FRAME: Duration = Duration::from_millis(16);
const DAMPING: f32 = 0.7;
const STIFFNESS: f32 = 0.05;
const MASS: f32 = 1.25;
const GROWTH_SMOOTHING: f32 = 0.12;
const LEAD_FRAMES: f32 = 9.0;
const MAX_LEAD: f32 = 32.0;

/// Ease-out curve for the glide: fast at first, settling at the end.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

/// Room to hold open below the content so a request at `prompt_top` can sit at the top of a `viewport` tall view.
/// `content` is the content's height without that room. It shrinks to zero as the answer grows.
pub fn held_room(viewport: f32, content: f32, prompt_top: f32) -> f32 {
    (viewport - (content - prompt_top)).max(0.0)
}

/// Whether the jump button shows, given whether it showed before and how far the view is from the end.
pub fn jump_visible(was_visible: bool, distance_from_end: f32) -> bool {
    if distance_from_end > JUMP_SHOW_DISTANCE {
        true
    } else if distance_from_end <= JUMP_HIDE_DISTANCE {
        false
    } else {
        was_visible
    }
}

/// The spring that follows a growing answer's end.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FollowSpring {
    pub offset: f32,
    pub velocity: f32,
    /// How fast the end has been growing, per frame, smoothed.
    pub growth: f32,
    last_end: Option<f32>,
}

impl FollowSpring {
    pub fn new(offset: f32) -> Self {
        Self {
            offset,
            ..Self::default()
        }
    }

    /// Advances one frame towards `end`, the largest offset the content allows. It aims a little above a growing end,
    /// more the faster it grows, and never moves up.
    pub fn step(self, end: f32) -> Self {
        let grown = self.last_end.map_or(0.0, |last| (end - last).max(0.0));
        let growth = self.growth + (grown - self.growth) * GROWTH_SMOOTHING;
        let target = end - (growth * LEAD_FRAMES).min(MAX_LEAD);
        let distance = (target - self.offset).max(0.0);
        let velocity = self.velocity * DAMPING + distance * STIFFNESS / MASS;
        Self {
            offset: (self.offset + velocity).min(end),
            velocity,
            growth,
            last_end: Some(end),
        }
    }

    /// Whether the spring has reached a resting end.
    pub fn settled(&self, end: f32) -> bool {
        end - self.offset < 0.5 && self.velocity < 0.5 && self.growth < 0.1
    }
}

/// View state of the conversation scroll, kept by the agent panel.
#[derive(Clone, Copy, PartialEq)]
pub struct ConversationFollow {
    controller: ScrollController,
    /// Whether the view follows the end. Scrolling away turns it off; scrolling back near the end turns it on.
    pub following: State<bool>,
    /// Room held open below the content for the last request that glided to the top.
    pub room: State<f32>,
    /// Offset of the last request from the top of the content, once measured.
    prompt_top: State<Option<f32>>,
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
}

/// Creates the conversation scroll state for `controller`.
pub fn use_conversation_follow(controller: ScrollController) -> ConversationFollow {
    ConversationFollow {
        controller,
        following: use_state(|| true),
        room: use_state(|| 0.0),
        prompt_top: use_state(|| None),
        generation: use_state(|| 0),
        spring_running: use_state(|| false),
        content: use_state(|| None),
        prompt_y: use_state(|| None),
        glide_pending: use_state(|| false),
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

    /// Starts a conversation fresh: no held room, following the end.
    pub fn reset(&mut self) {
        self.next_generation();
        self.glide_pending.set_if_modified(false);
        self.room.set_if_modified(0.0);
        self.prompt_top.set_if_modified(None);
        self.following.set_if_modified(true);
        self.controller.scroll_to(ScrollPosition::End, Direction::Vertical);
    }

    /// The user scrolled: let go of the end and stop any motion.
    pub fn release(&mut self) {
        self.next_generation();
        self.following.set_if_modified(false);
    }

    /// Re-attaches to the end when the user scrolled back within reach of it.
    pub fn restick_if_near_end(&mut self) {
        if *self.following.peek() {
            return;
        }
        let (offset, end) = self.offset_and_end();
        if end - offset <= STICK_DISTANCE {
            self.following.set(true);
        }
    }

    /// A request was sent. It glides to the top once both it and the content are laid out.
    pub fn request_glide(&mut self) {
        self.prompt_y.set(None);
        self.glide_pending.set(true);
    }

    /// The content was laid out at window y `y`, `height` tall with the held room.
    pub fn content_sized(&mut self, y: f32, height: f32) {
        self.content.set_if_modified(Some((y, height)));
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

    /// The request sits at `prompt_top` within the content, whose height without room is `content`. Holds room below
    /// it and glides it to the top.
    fn glide_prompt_to_top(&mut self, prompt_top: f32, content: f32) {
        let top = (prompt_top - PROMPT_TOP_GAP).max(0.0);
        self.prompt_top.set(Some(top));
        self.following.set(true);
        self.room.set(held_room(self.viewport_height(), content, top));
        self.glide_to(top);
    }

    /// The content changed height. Shrinks the held room as the answer grows into it, and once there is none left,
    /// lets the spring follow the end.
    fn content_resized(&mut self, content_with_room: f32) {
        let room = *self.room.peek();
        if let Some(prompt_top) = *self.prompt_top.peek() {
            let next = held_room(self.viewport_height(), content_with_room - room, prompt_top);
            if (next - room).abs() > 0.5 {
                self.room.set(next);
            }
            if next > 0.0 {
                return;
            }
        }
        if *self.following.peek() {
            self.follow_end();
        }
    }

    /// Glides to the end and follows it from there, for the jump button.
    pub fn jump_to_end(&mut self) {
        self.following.set(true);
        let (_, end) = self.offset_and_end();
        self.glide_to(end);
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

    fn follow_end(&mut self) {
        if *self.spring_running.peek() {
            return;
        }
        let generation = self.next_generation();
        self.spring_running.set(true);
        let mut controller = self.controller;
        let generation_state = self.generation;
        let mut spring_running = self.spring_running;
        let (offset, _) = self.offset_and_end();
        let this = *self;
        spawn(async move {
            let mut spring = FollowSpring::new(offset);
            loop {
                timer(FRAME).await;
                if *generation_state.peek() != generation {
                    break;
                }
                let (_, end) = this.offset_and_end();
                spring = spring.step(end);
                controller.scroll_to_y(-spring.offset.round() as i32);
                if spring.settled(end) {
                    spring_running.set(false);
                    break;
                }
            }
        });
    }
}

/// The "Jump to latest" button, shown once the reader scrolled far from the end. It reads the scroll position, so it lives in
/// its own component and the conversation above it does not redraw on every scroll frame.
#[derive(Clone, PartialEq)]
pub struct JumpToLatest {
    follow: ConversationFollow,
}

impl JumpToLatest {
    pub fn new(follow: ConversationFollow) -> Self {
        Self { follow }
    }
}

impl Component for JumpToLatest {
    fn render(&self) -> impl IntoElement {
        let theme = crate::ui::theme::use_theme();
        let mut visible = use_state(|| false);
        let mut follow = self.follow;

        let position = follow.controller.position();
        let bounds = *follow.controller.bounds.read();
        let end = bounds.map_or(0.0, |(content, viewport)| (content.height - viewport.height).max(0.0));
        let distance = end + position.y;
        // While the view follows the end, a glide or the held room can put the end far off for a moment; the button
        // is only for a reader who scrolled away.
        let next = !*follow.following.read() && jump_visible(*visible.peek(), distance);
        if next != *visible.peek() {
            visible.set(next);
        }
        follow.restick_if_near_end();

        if !*visible.read() {
            return rect().into_element();
        }

        let border = theme.color(crate::ui::theme::ThemeRole::BorderSubtle);
        // Above the conversation, which is deeper in the tree and would otherwise paint over it and take its clicks.
        rect()
            .layer(Layer::Relative(8))
            .position(Position::new_absolute().bottom(10.0).left(0.0))
            .width(Size::percent(100.0))
            .direction(Direction::Horizontal)
            .main_align(Alignment::center())
            .child(
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("Jump to latest")
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(5.0)
                    .height(Size::px(26.0))
                    .padding(Gaps::new(0.0, 11.0, 0.0, 9.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(theme.color(crate::ui::theme::ThemeRole::SurfaceRaised))
                    .border(Border::new().width(1.0).alignment(BorderAlignment::Inner).fill(border))
                    .shadow(theme.shadows.card.to_shadow())
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        follow.jump_to_end();
                    })
                    .child(
                        crate::ui::kit::Icon::new("arrow-down")
                            .size(11.0)
                            .role(crate::ui::theme::ThemeRole::Text),
                    )
                    .child(
                        label()
                            .text("Jump to latest")
                            .font_size(11.0)
                            .font_weight(FontWeight::MEDIUM)
                            .color(theme.color(crate::ui::theme::ThemeRole::Text)),
                    ),
            )
            .into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_lets_a_short_answer_sit_below_a_request_at_the_top() {
        // A 600 tall view, 1000 of content, and the request starts at 900: 500 of room puts it at the top.
        assert_eq!(held_room(600.0, 1000.0, 900.0), 500.0);
        // The answer grows by 300: the room shrinks by as much, so nothing moves.
        assert_eq!(held_room(600.0, 1300.0, 900.0), 200.0);
        // Once the answer passes the bottom of the view, no room is left.
        assert_eq!(held_room(600.0, 1600.0, 900.0), 0.0);
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
    fn spring_reaches_a_still_end_without_passing_it() {
        let mut spring = FollowSpring::new(0.0);
        for _ in 0..600 {
            spring = spring.step(400.0);
            assert!(spring.offset <= 400.0);
            if spring.settled(400.0) {
                break;
            }
        }
        assert!(spring.settled(400.0));
    }

    #[test]
    fn spring_stays_above_a_growing_end() {
        // Streaming at 2 px a frame, about 120 px a second.
        let mut spring = FollowSpring::new(0.0);
        let mut end = 0.0;
        for _ in 0..240 {
            end += 2.0;
            spring = spring.step(end);
            assert!(spring.offset <= end);
        }
        let behind = end - spring.offset;
        assert!(behind > 4.0 && behind < 60.0, "{behind}");
    }
}
