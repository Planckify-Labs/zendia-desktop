/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Glass surface container ([`GlassPanel`]) and rounded status/action capsule ([`Pill`]).

use crate::ui::kit::focus_trap::focus_ring_border;
use crate::ui::kit::icons::Icon;
use crate::ui::kit::interactive::use_interactive_feedback;
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use zendia_module_api::ui::UiTone;

/// Floating or docked panel with Zendia's blurred translucent glass material (or solid surface fallback).
#[derive(Clone, PartialEq)]
pub struct GlassPanel {
    glass: bool,
    padding: Option<Gaps>,
    spacing: f32,
    corner_radius: Option<f32>,
    width: Size,
    height: Size,
    children: Vec<Element>,
    key: DiffKey,
}

impl Default for GlassPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl ChildrenExt for GlassPanel {
    fn children_mut(&mut self) -> &mut Vec<Element> {
        &mut self.children
    }
}

impl KeyExt for GlassPanel {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl GlassPanel {
    pub fn new() -> Self {
        Self {
            glass: true,
            padding: None,
            spacing: 8.0,
            corner_radius: None,
            width: Size::auto(),
            height: Size::auto(),
            children: Vec::new(),
            key: DiffKey::None,
        }
    }

    pub fn glass(mut self, glass: bool) -> Self {
        self.glass = glass;
        self
    }

    pub fn padding(mut self, padding: impl Into<Gaps>) -> Self {
        self.padding = Some(padding.into());
        self
    }

    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = Some(radius);
        self
    }

    pub fn width(mut self, width: Size) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: Size) -> Self {
        self.height = height;
        self
    }
}

impl Component for GlassPanel {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let radius = self.corner_radius.unwrap_or(theme.radii.float);
        let padding = self.padding.unwrap_or_else(|| Gaps::new_all(theme.spacing.xl));

        let background = if self.glass {
            theme.color(ThemeRole::SurfaceGlass)
        } else {
            theme.color(ThemeRole::Surface)
        };
        let border_color = if self.glass {
            theme.glass.outline.to_color()
        } else {
            theme.color(ThemeRole::Border)
        };

        let sheen_gradient = LinearGradient::new()
            .angle(180.0)
            .stop((theme.glass.sheen_top.to_color(), 0.0))
            .stop((Color::TRANSPARENT, 100.0));

        let mut container = rect()
            .width(self.width.clone())
            .height(self.height.clone())
            .corner_radius(CornerRadius::new_all(radius))
            .background(background)
            .color(theme.color(ThemeRole::Text))
            .font_size(theme.font_sizes.md)
            .border(
                Border::new()
                    .width(if self.glass { 0.5 } else { 1.0 })
                    .alignment(BorderAlignment::Inner)
                    .fill(border_color),
            )
            .shadow(theme.shadows.float_highlight.to_shadow())
            .shadow(theme.shadows.float_ambient.to_shadow())
            .shadow(theme.shadows.float_drop.to_shadow())
            .overflow(Overflow::Clip);

        if self.glass {
            container = container
                .shadow(Shadow::new().inset().spread(0.5).color(theme.glass.outline.to_color()))
                .child(
                    // Fork 96px top specular sheen (`zendia.css:640-650`, `pointer-events: none`).
                    rect()
                        .position(Position::new_absolute().top(0.0).left(0.0).right(0.0))
                        .height(Size::px(96.0))
                        .background(sheen_gradient)
                        .interactive(Interactive::No),
                );
        }

        container = container.child(
            rect()
                .width(if self.width == Size::auto() {
                    Size::auto()
                } else {
                    Size::fill()
                })
                .height(if self.height == Size::auto() {
                    Size::auto()
                } else {
                    Size::fill()
                })
                .padding(padding)
                .spacing(self.spacing)
                .children(self.children.clone()),
        );

        if self.glass && theme.glass.blur > 0.0 {
            container.backdrop_blur(theme.glass.blur)
        } else {
            container
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Rounded-full pill used for floating agent indicators, statusline badges, and compact action capsules.
#[derive(Clone, PartialEq)]
pub struct Pill {
    label: String,
    icon: Option<String>,
    dot_role: Option<ThemeRole>,
    tone: UiTone,
    on_press: Option<EventHandler<Event<PressEventData>>>,
    key: DiffKey,
}

impl KeyExt for Pill {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Pill {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            dot_role: None,
            tone: UiTone::Neutral,
            on_press: None,
            key: DiffKey::None,
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn dot(mut self, role: ThemeRole) -> Self {
        self.dot_role = Some(role);
        self
    }

    pub fn tone(mut self, tone: UiTone) -> Self {
        self.tone = tone;
        self
    }

    pub fn on_press(mut self, handler: impl Into<EventHandler<Event<PressEventData>>>) -> Self {
        self.on_press = Some(handler.into());
        self
    }
}

impl Component for Pill {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let interactive = self.on_press.is_some();
        let feedback = use_interactive_feedback(interactive);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let accent_role = ThemeRole::from_tone(self.tone);
        let (rest_bg, hover_bg, pressed_bg, border_color, fg) = match self.tone {
            UiTone::Neutral => (
                theme.color(ThemeRole::SurfaceGlass),
                theme.rgba(ThemeRole::SurfaceRaised).to_color(),
                theme
                    .rgba(ThemeRole::SurfaceRaised)
                    .mix(theme.rgba(ThemeRole::Text), 0.10)
                    .to_color(),
                theme.glass.outline.to_color(),
                theme.color(ThemeRole::Text),
            ),
            _ => {
                let tint = theme.rgba(accent_role);
                (
                    tint.with_alpha(0.18).to_color(),
                    tint.with_alpha(0.28).to_color(),
                    tint.with_alpha(0.36).to_color(),
                    tint.with_alpha(0.50).to_color(),
                    tint.to_color(),
                )
            }
        };
        let bg = if interactive {
            feedback.background(rest_bg, hover_bg, pressed_bg)
        } else {
            rest_bg
        };

        let border = focus_ring_border(
            interactive && focus() == Focus::Keyboard,
            &theme,
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(border_color),
        );

        let on_press = self.on_press.clone();
        let on_press_key = self.on_press.clone();

        let pill = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(interactive)
            .a11y_role(if interactive {
                AccessibilityRole::Button
            } else {
                AccessibilityRole::Status
            })
            .a11y_alt(self.label.clone())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(theme.spacing.md)
            .padding(Gaps::new(6.0, 14.0, 6.0, 12.0))
            .corner_radius(CornerRadius::new_all(theme.radii.pill))
            .background(bg)
            .border(border)
            .shadow(theme.shadows.pill.to_shadow())
            .scale(if interactive { feedback.scale() } else { 1.0 })
            .maybe(interactive, |el| {
                el.cursor(CursorIcon::Pointer)
                    .on_pointer_enter(feedback.on_pointer_enter())
                    .on_pointer_leave(feedback.on_pointer_leave())
                    .on_pointer_down(feedback.on_pointer_down())
                    .on_global_pointer_up(feedback.on_pointer_up())
                    .on_press(move |e| {
                        a11y_id.request_focus();
                        if let Some(handler) = &on_press {
                            handler.call(e);
                        }
                    })
                    .on_key_down(move |e: Event<KeyboardEventData>| {
                        if (e.key == Key::Named(NamedKey::Enter) || matches!(&e.key, Key::Character(c) if c == " "))
                            && on_press_key.is_some()
                        {
                            e.stop_propagation();
                        }
                    })
            })
            .maybe_child(self.dot_role.map(|role| {
                rect()
                    .width(Size::px(8.0))
                    .height(Size::px(8.0))
                    .corner_radius(CornerRadius::new_all(theme.radii.pill))
                    .background(theme.color(role))
            }))
            .maybe_child(self.icon.as_ref().map(|name| {
                Icon::new(name.clone())
                    .size(14.0)
                    .role(if self.tone == UiTone::Neutral {
                        ThemeRole::Text
                    } else {
                        accent_role
                    })
            }))
            .child(
                label()
                    .text(self.label.clone())
                    .font_size(theme.font_sizes.sm)
                    .font_weight(FontWeight::MEDIUM)
                    .color(fg),
            );

        if theme.glass.blur > 0.0 && self.tone == UiTone::Neutral {
            pill.backdrop_blur(theme.glass.model_blur)
        } else {
            pill
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
