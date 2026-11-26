/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Shared native settings controls built with Zendia's theme and component kit, matching the fork's flat Zen
//! preferences palette, checkboxes, inline dropdown `menulist`, and smooth interactive feedback.

use crate::ui::chrome::menus::AnchorRect;
use crate::ui::kit::{
    GlassPanel, Icon, KitCheckbox, KitChip, KitSwitch, ListNavAction, ListNavConfig, Pill, PopupHorizontalAlign,
    PopupPresence, TypeaheadState, anchor_popup_origin, classify_list_key, focus_ring_border, use_interactive_feedback,
};
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, Rgba, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::model::{ServiceId, ServiceStatus};
use zendia_browser_core::modules::{service_down_summary, service_label};
use zendia_module_api::ui::UiTone;

/// Surface and border colors matching Zen's flat `about:preferences` canvas (`#1c1b22` dark / `#ffffff` light).
#[derive(Clone, Copy, Debug)]
pub struct SettingsPalette {
    pub canvas_bg: Color,
    pub sidebar_bg: Color,
    pub card_bg: Color,
    pub card_border: Color,
    pub input_bg: Color,
    pub input_border: Color,
    pub menulist_bg: Color,
    pub menulist_hover_bg: Color,
    pub row_hover_bg: Color,
    pub selected_pill_bg: Color,
    pub divider: Color,
}

impl SettingsPalette {
    pub fn for_scheme(scheme: ResolvedScheme) -> Self {
        match scheme {
            ResolvedScheme::Dark => Self {
                canvas_bg: Rgba::rgb(0x1c, 0x1b, 0x22).to_color(),
                sidebar_bg: Rgba::rgb(0x1c, 0x1b, 0x22).to_color(),
                card_bg: Rgba::from_rgb_alpha(255, 255, 255, 0.035).to_color(),
                card_border: Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
                input_bg: Rgba::rgb(0x16, 0x15, 0x1b).to_color(),
                input_border: Rgba::from_rgb_alpha(255, 255, 255, 0.16).to_color(),
                menulist_bg: Rgba::rgb(0x36, 0x36, 0x36).to_color(),
                menulist_hover_bg: Rgba::rgb(0x44, 0x44, 0x48).to_color(),
                row_hover_bg: Rgba::from_rgb_alpha(255, 255, 255, 0.06).to_color(),
                selected_pill_bg: Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
                divider: Rgba::from_rgb_alpha(255, 255, 255, 0.09).to_color(),
            },
            ResolvedScheme::Light => Self {
                canvas_bg: Rgba::rgb(255, 255, 255).to_color(),
                sidebar_bg: Rgba::rgb(255, 255, 255).to_color(),
                card_bg: Rgba::rgb(0xf8, 0xf8, 0xfb).to_color(),
                card_border: Rgba::from_rgb_alpha(20, 24, 40, 0.09).to_color(),
                input_bg: Rgba::rgb(255, 255, 255).to_color(),
                input_border: Rgba::from_rgb_alpha(20, 24, 40, 0.22).to_color(),
                menulist_bg: Rgba::rgb(0xf1, 0xf1, 0xf1).to_color(),
                menulist_hover_bg: Rgba::rgb(0xe5, 0xe5, 0xe8).to_color(),
                row_hover_bg: Rgba::from_rgb_alpha(20, 24, 40, 0.05).to_color(),
                selected_pill_bg: Rgba::rgb(0xea, 0xea, 0xf0).to_color(),
                divider: Rgba::from_rgb_alpha(20, 24, 40, 0.09).to_color(),
            },
        }
    }
}

/// Large page/section heading matching Zen's settings hierarchy.
pub fn section_heading(title: &str, subtitle: Option<&str>) -> Element {
    section_heading_with_action(title, subtitle, None)
}

/// Large page/section heading with an optional trailing action (e.g., `Reset defaults`).
pub fn section_heading_with_action(title: &str, subtitle: Option<&str>, trailing: Option<Element>) -> Element {
    let theme = use_theme();
    rect()
        .width(Size::fill())
        .spacing(4.0)
        .child(
            rect()
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .main_align(Alignment::SpaceBetween)
                .cross_align(Alignment::center())
                .spacing(12.0)
                .child(
                    rect().width(Size::flex(1.0)).child(
                        label()
                            .text(title.to_string())
                            .font_size(22.0)
                            .font_weight(FontWeight::BOLD)
                            .color(theme.color(ThemeRole::Text)),
                    ),
                )
                .maybe_child(trailing),
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

/// Flat Zen preferences group section (no heavy outer box) separated cleanly on the unified settings canvas.
pub fn settings_group_section(children: Vec<Element>) -> Element {
    let theme = use_theme();
    let palette = SettingsPalette::for_scheme(theme.scheme);
    rect()
        .width(Size::fill())
        .spacing(10.0)
        .child(
            rect()
                .width(Size::fill())
                .height(Size::px(1.0))
                .margin(Gaps::new(0.0, 0.0, 6.0, 0.0))
                .background(palette.divider),
        )
        .children(children)
        .into_element()
}

/// Rounded settings card grouping item lists (plugins, extensions, background services, search results).
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

/// Native Zen checkbox preference row (`KitCheckbox` on leading edge, optional `24px` child indent).
#[derive(Clone, PartialEq)]
pub struct SettingsCheckboxRow {
    pub title: String,
    pub description: Option<String>,
    pub checked: bool,
    pub indent: bool,
    pub disabled: bool,
    pub on_toggle: EventHandler<bool>,
    key: DiffKey,
}

impl KeyExt for SettingsCheckboxRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl SettingsCheckboxRow {
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

impl Component for SettingsCheckboxRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let enabled = !self.disabled;
        let feedback = use_interactive_feedback(enabled);
        let checked = self.checked;
        let on_toggle = self.on_toggle.clone();
        let on_row_toggle = self.on_toggle.clone();

        let bg = feedback.background(Color::TRANSPARENT, palette.row_hover_bg, palette.row_hover_bg);

        rect()
            .width(Size::fill())
            .padding(Gaps::new(5.0, 8.0, 5.0, if self.indent { 28.0 } else { 4.0 }))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::start())
            .spacing(10.0)
            .opacity(if enabled { 1.0 } else { 0.5 })
            .cursor(if enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            })
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .on_pointer_down(feedback.on_pointer_down())
            .on_global_pointer_up(feedback.on_pointer_up())
            .on_press(move |_| {
                if enabled {
                    on_row_toggle.call(!checked);
                }
            })
            .child(
                rect().padding(Gaps::new(2.0, 0.0, 0.0, 0.0)).child(
                    KitCheckbox::new(checked)
                        .disabled(!enabled)
                        .on_toggle(move |next| on_toggle.call(next)),
                ),
            )
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .spacing(2.0)
                    .child(
                        label()
                            .text(self.title.clone())
                            .font_size(13.0)
                            .font_weight(FontWeight::NORMAL)
                            .color(theme.color(ThemeRole::Text)),
                    )
                    .maybe_child(self.description.as_ref().map(|desc| {
                        label()
                            .text(desc.clone())
                            .font_size(11.5)
                            .color(theme.color(ThemeRole::TextMuted))
                    })),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Interactive toggle row (switch on trailing edge) with animated hover feedback.
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
        let enabled = !self.disabled;
        let feedback = use_interactive_feedback(enabled);
        let checked = self.checked;
        let on_toggle = self.on_toggle.clone();
        let on_row_toggle = self.on_toggle.clone();

        let bg = feedback.background(Color::TRANSPARENT, palette.row_hover_bg, palette.row_hover_bg);

        rect()
            .width(Size::fill())
            .padding(Gaps::new(6.0, 8.0, 6.0, if self.indent { 28.0 } else { 8.0 }))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(bg)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .main_align(Alignment::SpaceBetween)
            .cross_align(Alignment::center())
            .spacing(12.0)
            .opacity(if enabled { 1.0 } else { 0.5 })
            .cursor(if enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            })
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .on_pointer_down(feedback.on_pointer_down())
            .on_global_pointer_up(feedback.on_pointer_up())
            .on_press(move |_| {
                if enabled {
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
                    .disabled(!enabled)
                    .on_toggle(move |next| on_toggle.call(next)),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Inline dropdown preference row matching Zen's `menulist` (`background: light-dark(#f1f1f1, #363636)`,
/// `border-radius: 5px`, trailing chevron) with `PopupPresence` motion and `list_nav` keyboard navigation.
#[derive(Clone, PartialEq)]
pub struct SettingsDropdownRow {
    pub title: String,
    pub description: Option<String>,
    pub selected_value: String,
    pub options: Vec<(String, String)>,
    pub on_select: EventHandler<String>,
    key: DiffKey,
}

impl KeyExt for SettingsDropdownRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl SettingsDropdownRow {
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

impl Component for SettingsDropdownRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let feedback = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let mut open = use_state(|| false);
        let mut highlighted = use_state(|| 0usize);
        let mut typeahead = use_state(TypeaheadState::default);
        let mut trigger_area = use_state(|| None::<Area>);

        let is_open = *open.read();
        let selected_idx = self
            .options
            .iter()
            .position(|(v, _)| *v == self.selected_value)
            .unwrap_or(0);
        let current_label = self
            .options
            .get(selected_idx)
            .map(|(_, lbl)| lbl.clone())
            .unwrap_or_else(|| self.selected_value.clone());

        let area_val = *trigger_area.read();
        let popup_width = area_val.map(|a| a.width().max(260.0)).unwrap_or(280.0);
        let popup_height = (self.options.len() as f32 * 34.0 + 16.0).min(280.0);
        let origin = area_val
            .map(|a| {
                let anchor = AnchorRect {
                    x: a.min_x(),
                    y: a.min_y(),
                    width: a.width(),
                    height: a.height(),
                };
                anchor_popup_origin(
                    anchor,
                    (popup_width, popup_height),
                    (1440.0, 900.0),
                    PopupHorizontalAlign::Start,
                    4.0,
                    8.0,
                )
            })
            .unwrap_or((0.0, 0.0));

        let opts_key = self.options.clone();
        let on_select_key = self.on_select.clone();
        let on_popup_key_down = move |e: Event<KeyboardEventData>| {
            if !*open.peek() || opts_key.is_empty() {
                return;
            }
            let Some(action) = classify_list_key(&e, ListNavConfig::select()) else {
                return;
            };
            e.stop_propagation();
            e.prevent_default();
            let len = opts_key.len();
            let cur = (*highlighted.peek()).min(len - 1);
            match action {
                ListNavAction::MoveNext => highlighted.set((cur + 1).min(len - 1)),
                ListNavAction::MovePrev => highlighted.set(cur.saturating_sub(1)),
                ListNavAction::PageDown(step) => highlighted.set((cur + step).min(len - 1)),
                ListNavAction::PageUp(step) => highlighted.set(cur.saturating_sub(step)),
                ListNavAction::First => highlighted.set(0),
                ListNavAction::Last => highlighted.set(len - 1),
                ListNavAction::Dismiss => open.set(false),
                ListNavAction::Activate => {
                    if let Some((val, _)) = opts_key.get(cur) {
                        on_select_key.call(val.clone());
                    }
                    open.set(false);
                }
                ListNavAction::Typeahead(ch) => {
                    let mut ta = typeahead.write();
                    if let Some(found) = ta.push_and_find(
                        ch,
                        std::time::Instant::now(),
                        opts_key.len(),
                        Some(cur),
                        |i| opts_key.get(i).map(|(_, l)| l.clone()),
                        |_| true,
                    ) {
                        highlighted.set(found);
                    }
                }
                ListNavAction::OpenSubmenu | ListNavAction::CloseSubmenu => {}
            }
        };

        let btn_bg = feedback.background(
            palette.menulist_bg,
            palette.menulist_hover_bg,
            palette.menulist_hover_bg,
        );
        let btn_border = focus_ring_border(
            focus() == Focus::Keyboard || is_open,
            &theme,
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(palette.card_border),
        );

        let mut toggle_open = move || {
            let next = !*open.peek();
            if next {
                highlighted.set(selected_idx);
            }
            open.set(next);
        };
        let mut toggle_key = toggle_open;

        let option_rows: Vec<Element> = self
            .options
            .iter()
            .enumerate()
            .map(|(idx, (val, lbl))| {
                let is_selected = *val == self.selected_value;
                let is_hi = *highlighted.read() == idx;
                let val_clone = val.clone();
                let handler = self.on_select.clone();
                SettingsDropdownOptionRow {
                    label: lbl.clone(),
                    selected: is_selected,
                    highlighted: is_hi,
                    on_hover: EventHandler::new(move |_| highlighted.set_if_modified(idx)),
                    on_press: EventHandler::new(move |_| {
                        handler.call(val_clone.clone());
                        open.set(false);
                    }),
                    key: DiffKey::from(&format!("opt-{idx}")),
                }
                .into_element()
            })
            .collect();

        rect()
            .width(Size::fill())
            .padding(Gaps::new(6.0, 4.0, 6.0, 4.0))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .main_align(Alignment::SpaceBetween)
            .cross_align(Alignment::center())
            .spacing(16.0)
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
                rect()
                    .on_sized(move |e: Event<SizedEventData>| trigger_area.set_if_modified(Some(e.area)))
                    .child(
                        rect()
                            .a11y_id(a11y_id)
                            .a11y_focusable(true)
                            .a11y_role(AccessibilityRole::ComboBox)
                            .a11y_alt(format!("{}: {}", self.title, current_label))
                            .min_width(Size::px(220.0))
                            .max_width(Size::px(320.0))
                            .min_height(Size::px(32.0))
                            .padding(Gaps::new(6.0, 10.0, 6.0, 12.0))
                            .corner_radius(CornerRadius::new_all(5.0))
                            .background(btn_bg)
                            .border(btn_border)
                            .scale(feedback.scale())
                            .direction(Direction::Horizontal)
                            .content(Content::Flex)
                            .main_align(Alignment::SpaceBetween)
                            .cross_align(Alignment::center())
                            .spacing(10.0)
                            .cursor(CursorIcon::Pointer)
                            .on_pointer_enter(feedback.on_pointer_enter())
                            .on_pointer_leave(feedback.on_pointer_leave())
                            .on_pointer_down(feedback.on_pointer_down())
                            .on_global_pointer_up(feedback.on_pointer_up())
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                toggle_open();
                            })
                            .on_key_down(move |e: Event<KeyboardEventData>| {
                                if !*open.peek()
                                    && matches!(
                                        e.key,
                                        Key::Named(NamedKey::Enter | NamedKey::ArrowDown | NamedKey::ArrowUp)
                                    )
                                {
                                    e.stop_propagation();
                                    e.prevent_default();
                                    toggle_key();
                                }
                            })
                            .child(
                                label()
                                    .width(Size::flex(1.0))
                                    .text(current_label)
                                    .font_size(12.5)
                                    .font_weight(FontWeight::MEDIUM)
                                    .color(theme.color(ThemeRole::Text))
                                    .max_lines(1)
                                    .text_overflow(TextOverflow::Ellipsis),
                            )
                            .child(Icon::new("dart-down").size(12.0).role(ThemeRole::TextMuted)),
                    ),
            )
            .child(
                PopupPresence::anchored(is_open, origin)
                    .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                    .on_escape(move |_| open.set(false))
                    .on_backdrop_mouse_down(move |_| open.set(false))
                    .on_global_key_down(on_popup_key_down)
                    .child(
                        GlassPanel::new()
                            .width(Size::px(popup_width))
                            .padding(Gaps::new_all(5.0))
                            .spacing(2.0)
                            .corner_radius(10.0)
                            .children(option_rows),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SettingsDropdownOptionRow {
    label: String,
    selected: bool,
    highlighted: bool,
    on_hover: EventHandler<()>,
    on_press: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for SettingsDropdownOptionRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SettingsDropdownOptionRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let on_hover = self.on_hover.clone();
        let on_press = self.on_press.clone();

        let bg = if self.highlighted {
            theme.color(ThemeRole::Hover)
        } else if self.selected {
            theme.rgba(ThemeRole::Accent).with_alpha(0.12).to_color()
        } else {
            Color::TRANSPARENT
        };

        rect()
            .width(Size::fill())
            .min_height(Size::px(30.0))
            .padding(Gaps::new(5.0, 10.0, 5.0, 10.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .main_align(Alignment::SpaceBetween)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| on_hover.call(()))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_press.call(());
            })
            .child(
                label()
                    .width(Size::flex(1.0))
                    .text(self.label.clone())
                    .font_size(12.5)
                    .font_weight(if self.selected {
                        FontWeight::SEMI_BOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .color(if self.selected {
                        theme.color(ThemeRole::Accent)
                    } else {
                        theme.color(ThemeRole::Text)
                    })
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .maybe_child(
                self.selected
                    .then(|| Icon::new("check").size(12.0).role(ThemeRole::Accent)),
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
            .padding(Gaps::new(6.0, 4.0, 6.0, 4.0))
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

/// Compact clickable settings button styled to match Zen's action buttons (`Reload Plugins`, `Restart`, `New Space…`).
#[derive(Clone, PartialEq)]
pub struct SettingsButton {
    pub label: String,
    pub icon: Option<String>,
    pub primary: bool,
    pub danger: bool,
    pub disabled: bool,
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
            disabled: false,
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

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Component for SettingsButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let enabled = !self.disabled;
        let feedback = use_interactive_feedback(enabled);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_press = self.on_press.clone();

        let (rest_bg, hover_bg, pressed_bg, fg, icon_role, border_col) = if self.primary {
            let accent = theme.rgba(ThemeRole::Accent);
            (
                accent.to_color(),
                accent.with_alpha(0.90).to_color(),
                accent.with_alpha(0.78).to_color(),
                theme.color(ThemeRole::TextOnAccent),
                ThemeRole::TextOnAccent,
                Color::TRANSPARENT,
            )
        } else if self.danger {
            let danger = theme.rgba(ThemeRole::Danger);
            (
                danger.with_alpha(0.14).to_color(),
                danger.with_alpha(0.24).to_color(),
                danger.with_alpha(0.32).to_color(),
                theme.color(ThemeRole::Danger),
                ThemeRole::Danger,
                danger.with_alpha(0.38).to_color(),
            )
        } else {
            let (r, h, p, b) = match theme.scheme {
                ResolvedScheme::Dark => (
                    Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
                    Rgba::from_rgb_alpha(255, 255, 255, 0.14).to_color(),
                    Rgba::from_rgb_alpha(255, 255, 255, 0.19).to_color(),
                    Rgba::from_rgb_alpha(255, 255, 255, 0.12).to_color(),
                ),
                ResolvedScheme::Light => (
                    Rgba::from_rgb_alpha(20, 24, 40, 0.06).to_color(),
                    Rgba::from_rgb_alpha(20, 24, 40, 0.11).to_color(),
                    Rgba::from_rgb_alpha(20, 24, 40, 0.16).to_color(),
                    Rgba::from_rgb_alpha(20, 24, 40, 0.12).to_color(),
                ),
            };
            (r, h, p, theme.color(ThemeRole::Text), ThemeRole::Text, b)
        };

        let bg = feedback.background(rest_bg, hover_bg, pressed_bg);
        let border = focus_ring_border(
            enabled && focus() == Focus::Keyboard,
            &theme,
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(border_col),
        );

        rect()
            .a11y_id(a11y_id)
            .a11y_enabled(enabled)
            .a11y_focusable(enabled)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label.clone())
            .height(Size::px(30.0))
            .padding(Gaps::new(0.0, 12.0, 0.0, 12.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .border(border)
            .scale(if enabled { feedback.scale() } else { 1.0 })
            .opacity(if enabled { 1.0 } else { 0.45 })
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .main_align(Alignment::center())
            .spacing(6.0)
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
                if enabled {
                    on_press.call(());
                }
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if enabled && (e.key == Key::Named(NamedKey::Enter) || matches!(&e.key, Key::Character(c) if c == " "))
                {
                    e.stop_propagation();
                }
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
