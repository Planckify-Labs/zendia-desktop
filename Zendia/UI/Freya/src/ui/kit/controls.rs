/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Interactive themed controls wrapping Freya's built-in `Button`, `Chip`, `Switch`, and `Slider`.

use crate::ui::kit::focus_trap::focus_ring_border;
use crate::ui::kit::icons::Icon;
use crate::ui::kit::interactive::{mix_color, use_interactive_feedback};
use crate::ui::kit::tooltip::KitTooltip;
use crate::ui::theme::{ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use zendia_module_api::ui::UiTone;

/// Converts a domain value in `min..=max` into Freya's `0.0..=100.0` slider percentage.
pub fn normalize_slider_value(value: f32, min: f32, max: f32) -> f64 {
    if (max - min).abs() <= f32::EPSILON {
        return 0.0;
    }
    let clamped = value.clamp(min.min(max), min.max(max));
    ((clamped - min) as f64 / (max - min) as f64 * 100.0).clamp(0.0, 100.0)
}

/// Converts Freya's `0.0..=100.0` slider percentage back into a domain value in `min..=max`, snapping to `step` if set.
pub fn denormalize_slider_value(percent: f64, min: f32, max: f32, step: Option<f32>) -> f32 {
    if (max - min).abs() <= f32::EPSILON {
        return min;
    }
    let raw = min as f64 + (percent.clamp(0.0, 100.0) / 100.0) * (max - min) as f64;
    let snapped = match step.filter(|s| *s > 0.0 && s.is_finite()) {
        Some(step) => {
            let step = step as f64;
            min as f64 + ((raw - min as f64) / step).round() * step
        }
        None => raw,
    };
    (snapped as f32).clamp(min.min(max), min.max(max))
}

/// Converts a domain step into a percentage step for Freya's [`Slider`].
pub fn slider_step_percent(min: f32, max: f32, step: Option<f32>) -> Option<f64> {
    let span = (max - min).abs();
    let step = step?;
    if span <= f32::EPSILON || step <= 0.0 || !step.is_finite() {
        return None;
    }
    Some((step as f64 / span as f64) * 100.0)
}

/// Square icon button matching Zendia's panel/toolbar button styling, with animated hover (`120 ms`), active press
/// scale (`0.98`), keyboard focus ring, and optional delayed tooltip with shortcut badge.
#[derive(Clone, PartialEq)]
pub struct IconButton {
    icon: String,
    label: String,
    tooltip: Option<String>,
    shortcut: Option<String>,
    size: f32,
    icon_size: f32,
    active: bool,
    disabled: bool,
    role: ThemeRole,
    on_press: Option<EventHandler<Event<PressEventData>>>,
    key: DiffKey,
}

impl KeyExt for IconButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl IconButton {
    pub fn new(icon: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            icon: icon.into(),
            label: label.into(),
            tooltip: None,
            shortcut: None,
            size: 28.0,
            icon_size: 14.0,
            active: false,
            disabled: false,
            role: ThemeRole::TextMuted,
            on_press: None,
            key: DiffKey::None,
        }
    }

    /// Shows a delayed floating tooltip on hover (with an optional shortcut badge).
    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Sets a shortcut hint displayed inside the button's hover tooltip.
    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn icon_size(mut self, icon_size: f32) -> Self {
        self.icon_size = icon_size;
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn role(mut self, role: ThemeRole) -> Self {
        self.role = role;
        self
    }

    pub fn on_press(mut self, handler: impl Into<EventHandler<Event<PressEventData>>>) -> Self {
        self.on_press = Some(handler.into());
        self
    }
}

impl Component for IconButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let enabled = !self.disabled;
        let feedback = use_interactive_feedback(enabled);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let icon_role = if self.active {
            ThemeRole::Accent
        } else if feedback.is_hovered() {
            ThemeRole::Text
        } else {
            self.role
        };
        let rest_bg = if self.active {
            theme.rgba(ThemeRole::Accent).with_alpha(0.24).to_color()
        } else {
            Color::TRANSPARENT
        };
        let hover_bg = if self.active {
            theme.rgba(ThemeRole::Accent).with_alpha(0.32).to_color()
        } else {
            theme.color(ThemeRole::Hover)
        };
        let pressed_bg = if self.active {
            theme.rgba(ThemeRole::Accent).with_alpha(0.40).to_color()
        } else {
            theme.rgba(ThemeRole::Text).with_alpha(0.16).to_color()
        };
        let bg = feedback.background(rest_bg, hover_bg, pressed_bg);
        let border = focus_ring_border(focus() == Focus::Keyboard, &theme, Border::new());
        let on_press = self.on_press.clone();
        let on_key_press = self.on_press.clone();

        let button = rect()
            .a11y_id(a11y_id)
            .a11y_enabled(enabled)
            .a11y_focusable(enabled)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label.clone())
            .width(Size::px(self.size))
            .height(Size::px(self.size))
            .corner_radius(CornerRadius::new_all(theme.radii.sm))
            .background(bg)
            .border(border)
            .scale(feedback.scale())
            .opacity(if enabled { 1.0 } else { 0.42 })
            .center()
            .cursor(if enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            })
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .on_pointer_down(feedback.on_pointer_down())
            .on_global_pointer_up(feedback.on_pointer_up())
            .on_press(move |e| {
                if enabled && let Some(handler) = &on_press {
                    handler.call(e);
                }
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if enabled
                    && (e.key == Key::Named(NamedKey::Enter) || matches!(&e.key, Key::Character(c) if c == " "))
                    && let Some(_handler) = &on_key_press
                {
                    // Accessibility press events are also dispatched by Freya on Enter/Space when focusable.
                    e.stop_propagation();
                }
            })
            .child(Icon::new(self.icon.clone()).size(self.icon_size).role(icon_role));

        let tooltip_text = self.tooltip.as_deref().unwrap_or(self.label.as_str());
        if self.tooltip.is_some() || self.shortcut.is_some() {
            KitTooltip::new(tooltip_text)
                .maybe_shortcut(self.shortcut.clone())
                .enabled(enabled)
                .child(button)
                .into_element()
        } else {
            button.into_element()
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Compact selectable chip styled with Zendia's theme roles, wrapping Freya's [`Chip`].
#[derive(Clone, PartialEq)]
pub struct KitChip {
    label: String,
    icon: Option<String>,
    selected: bool,
    disabled: bool,
    tone: UiTone,
    on_press: Option<EventHandler<Event<PressEventData>>>,
    key: DiffKey,
}

impl KeyExt for KitChip {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl KitChip {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            selected: false,
            disabled: false,
            tone: UiTone::Neutral,
            on_press: None,
            key: DiffKey::None,
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
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

impl Component for KitChip {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let enabled = !self.disabled;
        let feedback = use_interactive_feedback(enabled);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let accent_role = if self.tone == UiTone::Neutral {
            ThemeRole::Accent
        } else {
            ThemeRole::from_tone(self.tone)
        };
        let accent = theme.rgba(accent_role);
        let text_role = if self.selected { accent_role } else { ThemeRole::Text };

        let rest_bg = if self.selected {
            accent.with_alpha(0.22).to_color()
        } else {
            theme.color(ThemeRole::SurfaceRaised)
        };
        let hover_bg = if self.selected {
            accent.with_alpha(0.30).to_color()
        } else {
            theme.rgba(ThemeRole::SurfaceRaised).mix(accent, 0.14).to_color()
        };
        let pressed_bg = if self.selected {
            accent.with_alpha(0.36).to_color()
        } else {
            theme.rgba(ThemeRole::SurfaceRaised).mix(accent, 0.22).to_color()
        };
        let bg = feedback.background(rest_bg, hover_bg, pressed_bg);

        let rest_border = if self.selected {
            accent.with_alpha(0.65).to_color()
        } else {
            theme.color(ThemeRole::BorderSubtle)
        };
        let border_col = mix_color(
            rest_border,
            accent.with_alpha(0.55).to_color(),
            feedback.hover_progress(),
        );
        let border = focus_ring_border(
            focus() == Focus::Keyboard && enabled,
            &theme,
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(border_col),
        );

        let on_press = self.on_press.clone();
        let on_press_key = self.on_press.clone();
        let interactive = enabled && self.on_press.is_some();

        rect()
            .a11y_id(a11y_id)
            .a11y_enabled(enabled)
            .a11y_focusable(interactive)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label.clone())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .main_align(Alignment::center())
            .spacing(5.0)
            .padding(Gaps::new(4.0, 10.0, 4.0, 10.0))
            .corner_radius(CornerRadius::new_all(theme.radii.pill))
            .background(bg)
            .border(border)
            .scale(if interactive { feedback.scale() } else { 1.0 })
            .opacity(if enabled { 1.0 } else { 0.45 })
            .cursor(if interactive {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            })
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .on_pointer_down(feedback.on_pointer_down())
            .on_global_pointer_up(feedback.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| {
                if enabled && let Some(handler) = &on_press {
                    handler.call(e);
                }
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if enabled
                    && (e.key == Key::Named(NamedKey::Enter) || matches!(&e.key, Key::Character(c) if c == " "))
                    && on_press_key.is_some()
                {
                    e.stop_propagation();
                }
            })
            .maybe_child(
                self.icon
                    .as_ref()
                    .map(|name| Icon::new(name.clone()).size(13.0).role(text_role)),
            )
            .child(
                label()
                    .text(self.label.clone())
                    .font_size(theme.font_sizes.sm)
                    .font_weight(FontWeight::MEDIUM)
                    .color(theme.color(text_role)),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Toggle switch styled to Zendia's theme and 36×20 proportions, built on Freya's [`Switch`].
#[derive(Clone, PartialEq)]
pub struct KitSwitch {
    label: Option<String>,
    checked: bool,
    disabled: bool,
    on_toggle: Option<EventHandler<bool>>,
    key: DiffKey,
}

impl KeyExt for KitSwitch {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl KitSwitch {
    pub fn new(checked: bool) -> Self {
        Self {
            label: None,
            checked,
            disabled: false,
            on_toggle: None,
            key: DiffKey::None,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_toggle(mut self, handler: impl Into<EventHandler<bool>>) -> Self {
        self.on_toggle = Some(handler.into());
        self
    }
}

impl Component for KitSwitch {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let enabled = !self.disabled;
        let feedback = use_interactive_feedback(enabled);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let checked = self.checked;

        let thumb_anim = use_animation_with_dependencies(&checked, move |conf, is_on| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *is_on { (0.0, 16.0) } else { (16.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::SWITCH_THUMB)
                .ease(Ease::Out)
                .function(motion::switch_spring_fn())
        });
        let bg_anim = use_animation_with_dependencies(&checked, move |conf, is_on| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *is_on { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::SWITCH_BG)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let track_off = theme.rgba(ThemeRole::Text).with_alpha(0.22).to_color();
        let track_on = theme.color(ThemeRole::Accent);
        let track_bg = mix_color(track_off, track_on, bg_anim.get().value().clamp(0.0, 1.0));
        let thumb_off = if theme.scheme.is_dark() {
            theme.color(ThemeRole::Text)
        } else {
            Color::WHITE
        };
        let thumb_on = theme.color(ThemeRole::TextOnAccent);
        let thumb_bg = mix_color(thumb_off, thumb_on, bg_anim.get().value().clamp(0.0, 1.0));
        let thumb_x = thumb_anim.get().value();

        let on_toggle_press = self.on_toggle.clone();
        let on_toggle_key = self.on_toggle.clone();
        let border = focus_ring_border(focus() == Focus::Keyboard, &theme, Border::new());

        let switch = rect()
            .a11y_id(a11y_id)
            .a11y_enabled(enabled)
            .a11y_focusable(enabled)
            .a11y_role(AccessibilityRole::Switch)
            .a11y_alt(self.label.clone().unwrap_or_else(|| "Toggle switch".to_string()))
            .width(Size::px(36.0))
            .height(Size::px(20.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .background(track_bg)
            .border(border)
            .scale(feedback.scale())
            .opacity(if enabled { 1.0 } else { 0.45 })
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .cursor(if enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            })
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .on_pointer_down(feedback.on_pointer_down())
            .on_global_pointer_up(feedback.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                if enabled && let Some(handler) = &on_toggle_press {
                    handler.call(!checked);
                }
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if enabled
                    && (e.key == Key::Named(NamedKey::Enter) || matches!(&e.key, Key::Character(c) if c == " "))
                    && let Some(handler) = &on_toggle_key
                {
                    e.stop_propagation();
                    e.prevent_default();
                    handler.call(!checked);
                }
            })
            .child(
                rect()
                    .position(Position::new_absolute().left(2.0 + thumb_x).top(2.0))
                    .width(Size::px(16.0))
                    .height(Size::px(16.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(thumb_bg)
                    .shadow(Shadow::new().y(1.0).blur(3.0).color(Color::from_argb(64, 0, 0, 0))),
            );

        if let Some(text) = &self.label {
            let on_label_toggle = self.on_toggle.clone();
            rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .spacing(theme.spacing.lg)
                .cursor(if enabled {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                })
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    if enabled && let Some(handler) = &on_label_toggle {
                        handler.call(!checked);
                    }
                })
                .child(switch)
                .child(
                    label()
                        .text(text.clone())
                        .font_size(theme.font_sizes.md)
                        .color(if self.disabled {
                            theme.color(ThemeRole::TextMuted)
                        } else {
                            theme.color(ThemeRole::Text)
                        }),
                )
                .into_element()
        } else {
            switch.into_element()
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Native Zendia checkbox (`16×16`, `4px` radius, `TextOnAccent` checkmark) matching Zen's settings checkboxes.
#[derive(Clone, PartialEq)]
pub struct KitCheckbox {
    label: Option<String>,
    checked: bool,
    disabled: bool,
    on_toggle: Option<EventHandler<bool>>,
    key: DiffKey,
}

impl KeyExt for KitCheckbox {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl KitCheckbox {
    pub fn new(checked: bool) -> Self {
        Self {
            label: None,
            checked,
            disabled: false,
            on_toggle: None,
            key: DiffKey::None,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_toggle(mut self, handler: impl Into<EventHandler<bool>>) -> Self {
        self.on_toggle = Some(handler.into());
        self
    }
}

impl Component for KitCheckbox {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let enabled = !self.disabled;
        let feedback = use_interactive_feedback(enabled);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let checked = self.checked;

        let accent = theme.rgba(ThemeRole::Accent);
        let rest_bg = if checked { accent.to_color() } else { Color::TRANSPARENT };
        let hover_bg = if checked {
            accent.with_alpha(0.88).to_color()
        } else {
            theme.color(ThemeRole::Hover)
        };
        let pressed_bg = if checked {
            accent.with_alpha(0.78).to_color()
        } else {
            theme.rgba(ThemeRole::Text).with_alpha(0.14).to_color()
        };
        let box_bg = feedback.background(rest_bg, hover_bg, pressed_bg);
        let box_border_col = if checked {
            accent.to_color()
        } else {
            mix_color(
                theme.color(ThemeRole::Border),
                theme.color(ThemeRole::TextMuted),
                feedback.hover_progress(),
            )
        };
        let box_border = focus_ring_border(
            focus() == Focus::Keyboard && enabled,
            &theme,
            Border::new()
                .width(1.5)
                .alignment(BorderAlignment::Inner)
                .fill(box_border_col),
        );

        let on_toggle_press = self.on_toggle.clone();
        let on_toggle_key = self.on_toggle.clone();

        let checkbox_box = rect()
            .a11y_id(a11y_id)
            .a11y_enabled(enabled)
            .a11y_focusable(enabled)
            .a11y_role(AccessibilityRole::CheckBox)
            .a11y_alt(self.label.clone().unwrap_or_else(|| "Checkbox".to_string()))
            .width(Size::px(16.0))
            .height(Size::px(16.0))
            .corner_radius(CornerRadius::new_all(4.0))
            .background(box_bg)
            .border(box_border)
            .scale(feedback.scale())
            .opacity(if enabled { 1.0 } else { 0.45 })
            .center()
            .cursor(if enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            })
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .on_pointer_down(feedback.on_pointer_down())
            .on_global_pointer_up(feedback.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                if enabled && let Some(handler) = &on_toggle_press {
                    handler.call(!checked);
                }
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if enabled
                    && (e.key == Key::Named(NamedKey::Enter) || matches!(&e.key, Key::Character(c) if c == " "))
                    && let Some(handler) = &on_toggle_key
                {
                    e.stop_propagation();
                    e.prevent_default();
                    handler.call(!checked);
                }
            })
            .maybe_child(checked.then(|| Icon::new("check").size(11.0).role(ThemeRole::TextOnAccent)));

        if let Some(text) = &self.label {
            let on_label_toggle = self.on_toggle.clone();
            rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .spacing(8.0)
                .cursor(if enabled {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                })
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    if enabled && let Some(handler) = &on_label_toggle {
                        handler.call(!checked);
                    }
                })
                .child(checkbox_box)
                .child(
                    label()
                        .text(text.clone())
                        .font_size(theme.font_sizes.md)
                        .color(if self.disabled {
                            theme.color(ThemeRole::TextMuted)
                        } else {
                            theme.color(ThemeRole::Text)
                        }),
                )
                .into_element()
        } else {
            checkbox_box.into_element()
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Slider styled to Zendia's theme roles, wrapping Freya's [`Slider`] with domain min/max/step mapping.
#[derive(Clone, PartialEq)]
pub struct KitSlider {
    label: Option<String>,
    value: f32,
    min: f32,
    max: f32,
    step: Option<f32>,
    disabled: bool,
    width: Size,
    accent_role: ThemeRole,
    on_change: Option<EventHandler<f32>>,
    key: DiffKey,
}

impl KeyExt for KitSlider {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl KitSlider {
    pub fn new(value: f32) -> Self {
        Self {
            label: None,
            value,
            min: 0.0,
            max: 100.0,
            step: None,
            disabled: false,
            width: Size::fill(),
            accent_role: ThemeRole::Accent,
            on_change: None,
            key: DiffKey::None,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn range(mut self, min: f32, max: f32) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    pub fn step(mut self, step: Option<f32>) -> Self {
        self.step = step;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn width(mut self, width: Size) -> Self {
        self.width = width;
        self
    }

    pub fn accent_role(mut self, role: ThemeRole) -> Self {
        self.accent_role = role;
        self
    }

    pub fn on_change(mut self, handler: impl Into<EventHandler<f32>>) -> Self {
        self.on_change = Some(handler.into());
        self
    }
}

impl Component for KitSlider {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let min = self.min;
        let max = self.max;
        let step = self.step;
        let on_change = self.on_change.clone();

        let pct = normalize_slider_value(self.value, min, max);
        let mut slider = Slider::new(move |next_pct: f64| {
            let next_value = denormalize_slider_value(next_pct, min, max, step);
            if let Some(handler) = &on_change {
                handler.call(next_value);
            }
        })
        .value(pct)
        .size(self.width.clone())
        .enabled(!self.disabled)
        .background(theme.rgba(ThemeRole::Border).with_alpha(0.45).to_color())
        .thumb_background(theme.color(ThemeRole::SurfaceRaised))
        .thumb_inner_background(theme.color(self.accent_role))
        .border_fill(theme.color(ThemeRole::FocusRing))
        .cursor(if self.disabled {
            CursorIcon::Default
        } else {
            CursorIcon::Pointer
        });

        if let Some(step_pct) = slider_step_percent(min, max, step) {
            slider = slider.step(step_pct);
        }

        let opacity = if self.disabled { 0.45 } else { 1.0 };

        if let Some(text) = &self.label {
            rect()
                .width(self.width.clone())
                .spacing(theme.spacing.xs)
                .opacity(opacity)
                .child(
                    label()
                        .text(text.clone())
                        .font_size(theme.font_sizes.sm)
                        .color(theme.color(ThemeRole::TextMuted)),
                )
                .child(slider)
                .into_element()
        } else {
            rect()
                .width(self.width.clone())
                .opacity(opacity)
                .child(slider)
                .into_element()
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_normalization_and_stepping_round_trip() {
        assert!((normalize_slider_value(0.5, 0.0, 1.0) - 50.0).abs() < 1e-6);
        assert!((normalize_slider_value(-5.0, 0.0, 10.0) - 0.0).abs() < 1e-6);
        assert!((normalize_slider_value(15.0, 0.0, 10.0) - 100.0).abs() < 1e-6);

        assert!((denormalize_slider_value(50.0, 0.0, 1.0, None) - 0.5).abs() < 1e-5);
        assert!((denormalize_slider_value(37.0, 0.0, 4.0, Some(1.0)) - 1.0).abs() < 1e-5);
        assert!((denormalize_slider_value(63.0, 0.0, 4.0, Some(1.0)) - 3.0).abs() < 1e-5);
        assert_eq!(slider_step_percent(0.0, 4.0, Some(1.0)), Some(25.0));
        assert_eq!(slider_step_percent(5.0, 5.0, Some(1.0)), None);
    }
}
