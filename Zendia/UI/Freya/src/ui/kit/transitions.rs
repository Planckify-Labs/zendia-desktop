/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Fade and slide visibility transitions using Zendia's motion timings and easing curves.

use crate::ui::theme::motion;
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;

/// Opacity fade transition using Zendia's `FLOAT_IN` / `FLOAT_OUT` timings and cubic-bezier curves.
#[derive(Clone, PartialEq)]
pub struct FadeTransition {
    visible: bool,
    enter_duration: Duration,
    exit_duration: Duration,
    children: Vec<Element>,
    key: DiffKey,
}

impl KeyExt for FadeTransition {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ChildrenExt for FadeTransition {
    fn children_mut(&mut self) -> &mut Vec<Element> {
        &mut self.children
    }
}

impl FadeTransition {
    pub fn new(visible: bool) -> Self {
        Self {
            visible,
            enter_duration: motion::FLOAT_CONTENT_IN,
            exit_duration: motion::FLOAT_OUT,
            children: Vec::new(),
            key: DiffKey::None,
        }
    }

    pub fn enter_duration(mut self, duration: Duration) -> Self {
        self.enter_duration = duration;
        self
    }

    pub fn exit_duration(mut self, duration: Duration) -> Self {
        self.exit_duration = duration;
        self
    }
}

impl Component for FadeTransition {
    fn render(&self) -> impl IntoElement {
        let enter_duration = self.enter_duration;
        let exit_duration = self.exit_duration;

        let anim = use_animation_with_dependencies(&self.visible, move |conf, visible| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            if *visible {
                AnimNum::new(0.0, 1.0)
                    .duration(enter_duration)
                    .ease(Ease::Out)
                    .function(motion::ease_out_fn())
            } else {
                AnimNum::new(1.0, 0.0)
                    .duration(exit_duration)
                    .ease(Ease::In)
                    .function(motion::ease_in_fn())
            }
        });

        let opacity = anim.get().value();
        let should_render = self.visible || opacity > 0.01;

        rect().maybe_child(should_render.then(|| rect().opacity(opacity).children(self.children.clone())))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Direction from which a [`SlideTransition`] enters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SlideDirection {
    #[default]
    Up,
    Down,
    Left,
    Right,
}

impl SlideDirection {
    /// Returns the starting `(offset_x, offset_y)` displacement for entering from this direction.
    pub fn start_offset(self, distance: f32) -> (f32, f32) {
        match self {
            Self::Up => (0.0, distance),
            Self::Down => (0.0, -distance),
            Self::Left => (distance, 0.0),
            Self::Right => (-distance, 0.0),
        }
    }
}

/// Combined opacity + directional slide transition using Zendia's spring / ease-out curves and tab/float timings.
#[derive(Clone, PartialEq)]
pub struct SlideTransition {
    visible: bool,
    direction: SlideDirection,
    distance: f32,
    enter_duration: Duration,
    exit_duration: Duration,
    children: Vec<Element>,
    key: DiffKey,
}

impl KeyExt for SlideTransition {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ChildrenExt for SlideTransition {
    fn children_mut(&mut self) -> &mut Vec<Element> {
        &mut self.children
    }
}

impl SlideTransition {
    pub fn new(visible: bool, direction: SlideDirection) -> Self {
        Self {
            visible,
            direction,
            distance: motion::TAB_SLIDE_PX,
            enter_duration: motion::TAB_SLIDE,
            exit_duration: motion::FLOAT_OUT,
            children: Vec::new(),
            key: DiffKey::None,
        }
    }

    pub fn distance(mut self, distance: f32) -> Self {
        self.distance = distance;
        self
    }

    pub fn enter_duration(mut self, duration: Duration) -> Self {
        self.enter_duration = duration;
        self
    }

    pub fn exit_duration(mut self, duration: Duration) -> Self {
        self.exit_duration = duration;
        self
    }
}

impl Component for SlideTransition {
    fn render(&self) -> impl IntoElement {
        let direction = self.direction;
        let distance = self.distance;
        let enter_duration = self.enter_duration;
        let exit_duration = self.exit_duration;
        let (start_x, start_y) = direction.start_offset(distance);

        let anim = use_animation_with_dependencies(&(self.visible, direction), move |conf, (visible, _)| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            if *visible {
                (
                    AnimNum::new(start_x, 0.0)
                        .duration(enter_duration)
                        .ease(Ease::Out)
                        .function(motion::spring_fn()),
                    AnimNum::new(start_y, 0.0)
                        .duration(enter_duration)
                        .ease(Ease::Out)
                        .function(motion::spring_fn()),
                    AnimNum::new(motion::TAB_FADE_FROM, 1.0)
                        .duration(motion::TAB_FADE)
                        .ease(Ease::Out)
                        .function(motion::ease_out_fn()),
                )
            } else {
                (
                    AnimNum::new(0.0, start_x * 0.4)
                        .duration(exit_duration)
                        .ease(Ease::In)
                        .function(motion::ease_in_fn()),
                    AnimNum::new(0.0, start_y * 0.4)
                        .duration(exit_duration)
                        .ease(Ease::In)
                        .function(motion::ease_in_fn()),
                    AnimNum::new(1.0, 0.0)
                        .duration(exit_duration)
                        .ease(Ease::In)
                        .function(motion::ease_in_fn()),
                )
            }
        });

        let (offset_x, offset_y, opacity) = anim.get().value();
        let should_render = self.visible || opacity > 0.01;

        rect().maybe_child(should_render.then(|| {
            rect()
                .offset_x(offset_x)
                .offset_y(offset_y)
                .opacity(opacity)
                .children(self.children.clone())
        }))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slide_direction_offsets_match_entry_edge() {
        assert_eq!(SlideDirection::Up.start_offset(14.0), (0.0, 14.0));
        assert_eq!(SlideDirection::Down.start_offset(14.0), (0.0, -14.0));
        assert_eq!(SlideDirection::Left.start_offset(14.0), (14.0, 0.0));
        assert_eq!(SlideDirection::Right.start_offset(14.0), (-14.0, 0.0));
    }
}
