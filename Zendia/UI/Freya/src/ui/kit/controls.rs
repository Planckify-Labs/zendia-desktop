/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Interactive themed controls wrapping Freya's built-in `Button`, `Chip`, `Switch`, and `Slider`.

use crate::ui::kit::icons::Icon;
use crate::ui::theme::{ThemeRole, use_theme};
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

/// Square icon button matching Zendia's panel/toolbar button styling, built on Freya's [`Button`].
#[derive(Clone, PartialEq)]
pub struct IconButton {
    icon: String,
    label: String,
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
            size: 28.0,
            icon_size: 14.0,
            active: false,
            disabled: false,
            role: ThemeRole::TextMuted,
            on_press: None,
            key: DiffKey::None,
        }
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
        let icon_role = if self.active { ThemeRole::Accent } else { self.role };
        let background = if self.active {
            theme.rgba(ThemeRole::Accent).with_alpha(0.24).to_color()
        } else {
            Color::TRANSPARENT
        };
        let hover_background = if self.active {
            theme.rgba(ThemeRole::Accent).with_alpha(0.32).to_color()
        } else {
            theme.color(ThemeRole::Hover)
        };

        let mut button = Button::new()
            .flat()
            .compact()
            .enabled(!self.disabled)
            .width(Size::px(self.size))
            .height(Size::px(self.size))
            .padding(Gaps::new_all(0.0))
            .corner_radius(CornerRadius::new_all(theme.radii.sm))
            .background(background)
            .hover_background(hover_background)
            .border_fill(Color::TRANSPARENT)
            .focus_border_fill(theme.color(ThemeRole::FocusRing))
            .color(theme.color(icon_role))
            .cursor(CursorIcon::Pointer)
            .child(
                rect()
                    .a11y_alt(self.label.clone())
                    .center()
                    .child(Icon::new(self.icon.clone()).size(self.icon_size).role(icon_role)),
            );

        if let Some(on_press) = &self.on_press {
            button = button.on_press(on_press.clone());
        }
        button
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
        let accent_role = if self.tone == UiTone::Neutral {
            ThemeRole::Accent
        } else {
            ThemeRole::from_tone(self.tone)
        };
        let accent = theme.rgba(accent_role);
        let text_role = if self.selected { accent_role } else { ThemeRole::Text };

        let mut chip = Chip::new()
            .selected(self.selected)
            .enabled(!self.disabled)
            .background(theme.color(ThemeRole::SurfaceRaised))
            .hover_background(theme.rgba(ThemeRole::SurfaceRaised).mix(accent, 0.14).to_color())
            .selected_background(accent.with_alpha(0.22).to_color())
            .border_fill(theme.color(ThemeRole::BorderSubtle))
            .hover_border_fill(accent.with_alpha(0.45).to_color())
            .selected_border_fill(accent.with_alpha(0.65).to_color())
            .focus_border_fill(theme.color(ThemeRole::FocusRing))
            .color(theme.color(ThemeRole::Text))
            .hover_color(theme.color(ThemeRole::Text))
            .selected_color(accent.to_color())
            .selected_icon_fill(accent.to_color())
            .hover_icon_fill(accent.to_color())
            .corner_radius(CornerRadius::new_all(theme.radii.pill))
            .padding(Gaps::new(4.0, 10.0, 4.0, 10.0))
            .margin(0.0)
            .width(Size::auto())
            .height(Size::auto())
            .cursor(CursorIcon::Pointer)
            .maybe_child(
                self.icon
                    .as_ref()
                    .map(|name| Icon::new(name.clone()).size(13.0).role(text_role)),
            )
            .child(
                label()
                    .text(self.label.clone())
                    .font_size(theme.font_sizes.sm)
                    .font_weight(FontWeight::MEDIUM),
            );

        if let Some(on_press) = &self.on_press {
            chip = chip.on_press(on_press.clone());
        }
        chip
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
        let mut internal = use_state(|| self.checked);
        if *internal.peek() != self.checked {
            internal.set(self.checked);
        }

        let on_toggle = self.on_toggle.clone();
        let setter = WritableSetter::new(internal, move |next: bool| {
            internal.set(next);
            if let Some(handler) = &on_toggle {
                handler.call(next);
            }
        });

        let track_off = theme.rgba(ThemeRole::Text).with_alpha(0.22).to_color();
        let thumb_off = if theme.scheme.is_dark() {
            theme.color(ThemeRole::Text)
        } else {
            Color::WHITE
        };
        let thumb_on = theme.color(ThemeRole::TextOnAccent);

        let switch = Switch::new(setter)
            .enabled(!self.disabled)
            .background(track_off)
            .thumb_background(thumb_off)
            .toggled_background(theme.color(ThemeRole::Accent))
            .toggled_thumb_background(thumb_on)
            .focus_border_fill(theme.color(ThemeRole::FocusRing))
            .margin(Gaps::new_all(0.0))
            .width(36.0)
            .height(20.0)
            .padding(2.0)
            .thumb_size(16.0)
            .toggled_thumb_size(16.0)
            .pressed_thumb_size_offset(1.5)
            .thumb_offset(0.0)
            .toggled_thumb_offset(16.0)
            .cursor(CursorIcon::Pointer);

        if let Some(text) = &self.label {
            rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .spacing(theme.spacing.lg)
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
        .cursor(CursorIcon::Pointer);

        if let Some(step_pct) = slider_step_percent(min, max, step) {
            slider = slider.step(step_pct);
        }

        if let Some(text) = &self.label {
            rect()
                .width(self.width.clone())
                .spacing(theme.spacing.xs)
                .child(
                    label()
                        .text(text.clone())
                        .font_size(theme.font_sizes.sm)
                        .color(theme.color(ThemeRole::TextMuted)),
                )
                .child(slider)
                .into_element()
        } else {
            slider.into_element()
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
