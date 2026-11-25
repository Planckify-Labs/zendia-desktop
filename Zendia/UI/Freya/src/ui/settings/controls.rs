/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Shared native settings controls built with Zendia's theme and component kit.

use crate::ui::kit::{Icon, KitChip, KitSwitch, Pill};
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::model::{ServiceId, ServiceStatus};
use zendia_browser_core::modules::{service_down_summary, service_label};
use zendia_module_api::ui::UiTone;

/// Surface and border colors derived from the active theme for settings cards and inputs.
#[derive(Clone, Copy, Debug)]
pub struct SettingsPalette {
    pub canvas_bg: Color,
    pub sidebar_bg: Color,
    pub card_bg: Color,
    pub card_border: Color,
    pub input_bg: Color,
    pub input_border: Color,
    pub row_hover_bg: Color,
    pub selected_pill_bg: Color,
    pub divider: Color,
}

impl SettingsPalette {
    pub fn for_scheme(scheme: ResolvedScheme) -> Self {
        match scheme {
            ResolvedScheme::Dark => Self {
                canvas_bg: Rgba::rgb(26, 27, 38).to_color(),
                sidebar_bg: Rgba::rgb(22, 23, 33).to_color(),
                card_bg: Rgba::rgb(33, 36, 52).to_color(),
                card_border: Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
                input_bg: Rgba::rgb(20, 21, 30).to_color(),
                input_border: Rgba::from_rgb_alpha(255, 255, 255, 0.16).to_color(),
                row_hover_bg: Rgba::from_rgb_alpha(255, 255, 255, 0.06).to_color(),
                selected_pill_bg: Rgba::from_rgb_alpha(130, 170, 255, 0.16).to_color(),
                divider: Rgba::from_rgb_alpha(255, 255, 255, 0.09).to_color(),
            },
            ResolvedScheme::Light => Self {
                canvas_bg: Rgba::rgb(248, 248, 252).to_color(),
                sidebar_bg: Rgba::rgb(241, 242, 247).to_color(),
                card_bg: Rgba::rgb(255, 255, 255).to_color(),
                card_border: Rgba::from_rgb_alpha(20, 24, 40, 0.09).to_color(),
                input_bg: Rgba::rgb(255, 255, 255).to_color(),
                input_border: Rgba::from_rgb_alpha(20, 24, 40, 0.22).to_color(),
                row_hover_bg: Rgba::from_rgb_alpha(20, 24, 40, 0.05).to_color(),
                selected_pill_bg: Rgba::from_rgb_alpha(52, 92, 180, 0.12).to_color(),
                divider: Rgba::from_rgb_alpha(20, 24, 40, 0.09).to_color(),
            },
        }
    }
}

/// Large page/section heading matching Zen's settings hierarchy.
pub fn section_heading(title: &str, subtitle: Option<&str>) -> Element {
    let theme = use_theme();
    rect()
        .width(Size::fill())
        .spacing(4.0)
        .child(
            label()
                .text(title.to_string())
                .font_size(22.0)
                .font_weight(FontWeight::BOLD)
                .color(theme.color(ThemeRole::Text)),
        )
        .maybe_child(subtitle.map(|sub| {
            label()
                .text(sub.to_string())
                .font_size(theme.font_sizes.md)
                .color(theme.color(ThemeRole::TextMuted))
        }))
        .into_element()
}

/// Secondary subsection heading inside a settings group.
pub fn subsection_heading(title: &str, description: Option<&str>) -> Element {
    let theme = use_theme();
    rect()
        .width(Size::fill())
        .spacing(3.0)
        .child(
            label()
                .text(title.to_string())
                .font_size(14.5)
                .font_weight(FontWeight::SEMI_BOLD)
                .color(theme.color(ThemeRole::Text)),
        )
        .maybe_child(description.map(|desc| {
            label()
                .text(desc.to_string())
                .font_size(theme.font_sizes.sm)
                .color(theme.color(ThemeRole::TextMuted))
        }))
        .into_element()
}

/// Rounded settings card grouping related options cleanly.
pub fn settings_card(children: Vec<Element>) -> Element {
    let theme = use_theme();
    let palette = SettingsPalette::for_scheme(theme.scheme);
    rect()
        .width(Size::fill())
        .padding(Gaps::new(14.0, 16.0, 14.0, 16.0))
        .spacing(12.0)
        .corner_radius(CornerRadius::new_all(10.0))
        .background(palette.card_bg)
        .border(
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(palette.card_border),
        )
        .children(children)
        .into_element()
}

/// Interactive toggle row (switch on trailing edge) with its own hover/focus hooks.
#[derive(Clone, PartialEq)]
pub struct SettingsToggleRow {
    pub title: String,
    pub description: Option<String>,
    pub checked: bool,
    pub indent: bool,
    pub disabled: bool,
    pub on_toggle: EventHandler<bool>,
    key: DiffKey,
}

impl KeyExt for SettingsToggleRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl SettingsToggleRow {
    pub fn new(title: impl Into<String>, checked: bool, on_toggle: impl Into<EventHandler<bool>>) -> Self {
        Self {
            title: title.into(),
            description: None,
            checked,
            indent: false,
            disabled: false,
            on_toggle: on_toggle.into(),
            key: DiffKey::None,
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn indent(mut self, indent: bool) -> Self {
        self.indent = indent;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Component for SettingsToggleRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let mut hovered = use_state(|| false);
        let checked = self.checked;
        let disabled = self.disabled;
        let on_toggle = self.on_toggle.clone();
        let on_row_toggle = self.on_toggle.clone();

        rect()
            .width(Size::fill())
            .padding(Gaps::new(6.0, 8.0, 6.0, if self.indent { 28.0 } else { 8.0 }))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(if *hovered.read() && !disabled {
                palette.row_hover_bg
            } else {
                Color::TRANSPARENT
            })
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .main_align(Alignment::SpaceBetween)
            .cross_align(Alignment::center())
            .spacing(12.0)
            .opacity(if disabled { 0.5 } else { 1.0 })
            .cursor(if disabled {
                CursorIcon::Default
            } else {
                CursorIcon::Pointer
            })
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |_| {
                if !disabled {
                    on_row_toggle.call(!checked);
                }
            })
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .spacing(2.0)
                    .child(
                        label()
                            .text(self.title.clone())
                            .font_size(13.0)
                            .font_weight(FontWeight::MEDIUM)
                            .color(theme.color(ThemeRole::Text)),
                    )
                    .maybe_child(self.description.as_ref().map(|desc| {
                        label()
                            .text(desc.clone())
                            .font_size(11.5)
                            .color(theme.color(ThemeRole::TextMuted))
                    })),
            )
            .child(
                KitSwitch::new(checked)
                    .disabled(disabled)
                    .on_toggle(move |next| on_toggle.call(next)),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Interactive choice row rendering labeled options as selectable chips (compact and keyboard-accessible).
#[derive(Clone, PartialEq)]
pub struct SettingsChoiceRow {
    pub title: String,
    pub description: Option<String>,
    pub selected_value: String,
    pub options: Vec<(String, String)>,
    pub on_select: EventHandler<String>,
    key: DiffKey,
}

impl KeyExt for SettingsChoiceRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl SettingsChoiceRow {
    pub fn new(
        title: impl Into<String>,
        selected_value: impl Into<String>,
        options: Vec<(String, String)>,
        on_select: impl Into<EventHandler<String>>,
    ) -> Self {
        Self {
            title: title.into(),
            description: None,
            selected_value: selected_value.into(),
            options,
            on_select: on_select.into(),
            key: DiffKey::None,
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

impl Component for SettingsChoiceRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let on_select = self.on_select.clone();

        rect()
            .width(Size::fill())
            .padding(Gaps::new(6.0, 8.0, 6.0, 8.0))
            .spacing(8.0)
            .child(
                rect()
                    .width(Size::fill())
                    .spacing(2.0)
                    .child(
                        label()
                            .text(self.title.clone())
                            .font_size(13.0)
                            .font_weight(FontWeight::MEDIUM)
                            .color(theme.color(ThemeRole::Text)),
                    )
                    .maybe_child(self.description.as_ref().map(|desc| {
                        label()
                            .text(desc.clone())
                            .font_size(11.5)
                            .color(theme.color(ThemeRole::TextMuted))
                    })),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .spacing(6.0)
                    .children(self.options.iter().map(|(value, label_text)| {
                        let is_selected = *value == self.selected_value;
                        let val_clone = value.clone();
                        let handler = on_select.clone();
                        KitChip::new(label_text.clone())
                            .key(value.clone())
                            .selected(is_selected)
                            .on_press(move |_| handler.call(val_clone.clone()))
                    })),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Compact clickable settings button styled to match Zen's action buttons (`Import`, `Restart`, `New Space...`).
#[derive(Clone, PartialEq)]
pub struct SettingsButton {
    pub label: String,
    pub icon: Option<String>,
    pub primary: bool,
    pub danger: bool,
    pub on_press: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for SettingsButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl SettingsButton {
    pub fn new(label: impl Into<String>, on_press: impl Into<EventHandler<()>>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            primary: false,
            danger: false,
            on_press: on_press.into(),
            key: DiffKey::None,
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn primary(mut self, primary: bool) -> Self {
        self.primary = primary;
        self
    }

    pub fn danger(mut self, danger: bool) -> Self {
        self.danger = danger;
        self
    }
}

impl Component for SettingsButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_press = self.on_press.clone();

        let (bg, fg, icon_role, border_col) = if self.primary {
            let base = if *hovered.read() {
                theme.rgba(ThemeRole::Accent).with_alpha(0.9).to_color()
            } else {
                theme.color(ThemeRole::Accent)
            };
            (
                base,
                theme.color(ThemeRole::TextOnAccent),
                ThemeRole::TextOnAccent,
                Color::TRANSPARENT,
            )
        } else if self.danger {
            let base = if *hovered.read() {
                theme.rgba(ThemeRole::Danger).with_alpha(0.24).to_color()
            } else {
                theme.rgba(ThemeRole::Danger).with_alpha(0.14).to_color()
            };
            (
                base,
                theme.color(ThemeRole::Danger),
                ThemeRole::Danger,
                theme.rgba(ThemeRole::Danger).with_alpha(0.38).to_color(),
            )
        } else {
            let base = match (theme.scheme, *hovered.read()) {
                (ResolvedScheme::Dark, true) => Rgba::from_rgb_alpha(255, 255, 255, 0.14).to_color(),
                (ResolvedScheme::Dark, false) => Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
                (ResolvedScheme::Light, true) => Rgba::from_rgb_alpha(20, 24, 40, 0.11).to_color(),
                (ResolvedScheme::Light, false) => Rgba::from_rgb_alpha(20, 24, 40, 0.06).to_color(),
            };
            let border = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.12).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(20, 24, 40, 0.12).to_color(),
            };
            (base, theme.color(ThemeRole::Text), ThemeRole::Text, border)
        };

        let border = if focus() == Focus::Keyboard {
            Border::new()
                .width(1.5)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::FocusRing))
        } else {
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(border_col)
        };

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label.clone())
            .height(Size::px(30.0))
            .padding(Gaps::new(0.0, 12.0, 0.0, 12.0))
            .corner_radius(CornerRadius::new_all(7.0))
            .background(bg)
            .border(border)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .main_align(Alignment::center())
            .spacing(6.0)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_press.call(());
            })
            .maybe_child(
                self.icon
                    .as_ref()
                    .map(|icon_name| Icon::new(icon_name.clone()).size(13.5).role(icon_role)),
            )
            .child(
                label()
                    .text(self.label.clone())
                    .font_size(12.5)
                    .font_weight(FontWeight::MEDIUM)
                    .color(fg)
                    .max_lines(1),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Service-down or restarting banner rendered inside a settings group whose backing service is unavailable.
#[derive(Clone, PartialEq)]
pub struct ServiceStatusBanner {
    pub service: ServiceId,
    pub status: ServiceStatus,
    pub on_restart: EventHandler<()>,
}

impl Component for ServiceStatusBanner {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let is_restarting = matches!(self.status, ServiceStatus::Restarting | ServiceStatus::Starting);
        let role = if is_restarting {
            ThemeRole::Warning
        } else {
            ThemeRole::Danger
        };
        let tone = if is_restarting { UiTone::Warning } else { UiTone::Danger };
        let detail = match &self.status {
            ServiceStatus::Stopped { reason: Some(r) } => r.clone(),
            _ => service_down_summary(self.service).to_string(),
        };
        let title = if is_restarting {
            format!("{} is restarting…", service_label(self.service))
        } else {
            format!("{} is stopped", service_label(self.service))
        };
        let on_restart = self.on_restart.clone();

        rect()
            .width(Size::fill())
            .padding(Gaps::new(11.0, 14.0, 11.0, 14.0))
            .corner_radius(CornerRadius::new_all(10.0))
            .background(theme.rgba(role).with_alpha(0.13).to_color())
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.rgba(role).with_alpha(0.42).to_color()),
            )
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .main_align(Alignment::SpaceBetween)
            .cross_align(Alignment::center())
            .spacing(12.0)
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .spacing(10.0)
                    .child(Icon::new("warning").size(16.0).role(role))
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .spacing(2.0)
                            .child(
                                rect()
                                    .direction(Direction::Horizontal)
                                    .cross_align(Alignment::center())
                                    .spacing(8.0)
                                    .child(
                                        label()
                                            .text(title)
                                            .font_size(13.0)
                                            .font_weight(FontWeight::SEMI_BOLD)
                                            .color(theme.color(ThemeRole::Text)),
                                    )
                                    .child(Pill::new(if is_restarting { "Restarting" } else { "Stopped" }).tone(tone)),
                            )
                            .child(
                                label()
                                    .text(detail)
                                    .font_size(12.0)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            ),
                    ),
            )
            .child(SettingsButton::new("Restart", move |_| on_restart.call(())).icon("reload"))
    }
}
