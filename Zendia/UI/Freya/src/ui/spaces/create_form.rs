/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Sidebar space creation form (`zen-workspace-creation`): title, subtitle, dashed icon picker button + name input,
//! profile selector, Theme... button (opening the gradient generator), and Create Space / Cancel footer buttons.

use crate::ui::chrome::layout::ChromePalette;
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::spaces::state::SpaceEditingState;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, Rgba, ThemeRole, use_theme};
use freya::engine::prelude::{Canvas, Paint, PaintStyle, PathEffect, RRect, Rect as SkRect, SkColor};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::space_appearance::SPACE_PROFILES;
use zendia_browser_core::space_routing::SELECTABLE_SPACE_ICONS;

/// Full-height creation form mounted inside the sidebar in place of the tab list.
#[derive(Clone, PartialEq)]
pub struct SpaceCreationForm {
    pub state: SpaceEditingState,
}

impl Component for SpaceCreationForm {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let name_a11y_id = use_a11y();
        let mut theme_btn_hovered = use_state(|| false);
        let mut create_btn_hovered = use_state(|| false);
        let mut cancel_btn_hovered = use_state(|| false);
        let mut profile_btn_hovered = use_state(|| false);
        let mut icon_btn_area = use_state(|| (194.0f32, 224.0f32));
        let mut theme_btn_area = use_state(|| (194.0f32, 300.0f32));

        let editing = self.state;
        let current_name = editing.draft_name.read().clone();
        let current_icon = editing.draft_icon.read().clone();
        let current_profile = editing.draft_profile.read().clone();
        let can_create = !current_name.trim().is_empty();
        let icon_picker_open = *editing.icon_picker_open.read();
        let profile_menu_open = *editing.profile_menu_open.read();
        let draft_dots = editing.draft_appearance.read().dots.clone();

        let (field_bg, field_hover_bg, pill_bg, primary_disabled_bg, primary_disabled_fg, dashed_sk) =
            match theme.scheme {
                ResolvedScheme::Dark => (
                    Rgba::from_rgb_alpha(255, 255, 255, 0.065).to_color(),
                    Rgba::from_rgb_alpha(255, 255, 255, 0.11).to_color(),
                    Rgba::from_rgb_alpha(255, 255, 255, 0.11).to_color(),
                    Rgba::from_rgb_alpha(255, 255, 255, 0.16).to_color(),
                    Rgba::from_rgb_alpha(230, 232, 240, 0.48).to_color(),
                    SkColor::from_argb(135, 255, 255, 255),
                ),
                ResolvedScheme::Light => (
                    Rgba::from_rgb_alpha(0, 0, 0, 0.055).to_color(),
                    Rgba::from_rgb_alpha(0, 0, 0, 0.10).to_color(),
                    Rgba::from_rgb_alpha(0, 0, 0, 0.095).to_color(),
                    Rgba::from_rgb_alpha(126, 118, 146, 0.72).to_color(),
                    Rgba::from_rgb_alpha(255, 255, 255, 0.85).to_color(),
                    SkColor::from_argb(128, 0, 0, 0),
                ),
            };

        let submit_core = core.clone();
        let submit_action = move || {
            let trimmed = editing.draft_name.peek().trim().to_string();
            if trimmed.is_empty() {
                return;
            }
            let icon = editing.draft_icon.peek().trim().to_string();
            let profile = editing.draft_profile.peek().trim().to_string();
            let appearance = editing.draft_appearance.peek().clone();
            submit_core.invoke_or_log(Command::SpaceCreateFormSubmit {
                name: trimmed,
                icon: (!icon.is_empty()).then_some(icon),
                profile: (!profile.is_empty()).then_some(profile),
                appearance: Some(appearance),
            });
        };

        let cancel_core = core.clone();
        let cancel_action = move || {
            cancel_core.invoke_or_log(Command::SpaceCreateFormCancel);
        };

        let key_submit = submit_action.clone();
        let key_cancel = cancel_action.clone();
        let on_input_key = move |e: Event<KeyboardEventData>| match e.key {
            Key::Named(NamedKey::Enter) => {
                e.stop_propagation();
                e.prevent_default();
                key_submit();
                false
            }
            Key::Named(NamedKey::Escape) => {
                e.stop_propagation();
                e.prevent_default();
                key_cancel();
                false
            }
            _ => true,
        };

        let dashed_cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_dashed_square(canvas, ctx.size.width, ctx.size.height, dashed_sk);
            }
        });

        let icon_button = rect()
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Choose space icon")
            .width(Size::px(24.0))
            .height(Size::px(24.0))
            .background(Fill::Callback(dashed_cb))
            .center()
            .cursor(CursorIcon::Pointer)
            .on_sized(move |e: Event<SizedEventData>| {
                let next = (e.area.max_x() + 10.0, e.area.min_y() - 10.0);
                if *icon_btn_area.peek() != next {
                    icon_btn_area.set(next);
                }
            })
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                let mut anchor = editing.icon_picker_anchor;
                anchor.set(*icon_btn_area.peek());
                let mut open = editing.icon_picker_open;
                let next = !*open.peek();
                open.set(next);
                let mut prof = editing.profile_menu_open;
                prof.set_if_modified(false);
            })
            .maybe_child(
                (!current_icon.is_empty()).then(|| Icon::new(current_icon.clone()).size(14.0).role(ThemeRole::Text)),
            );

        let name_row = rect()
            .width(Size::fill())
            .height(Size::px(38.0))
            .padding(Gaps::new(0.0, 8.0, 0.0, 7.0))
            .spacing(8.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(8.0))
            .background(field_bg)
            .child(icon_button)
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .height(Size::fill())
                    .cross_align(Alignment::center())
                    .direction(Direction::Horizontal)
                    .child(
                        Input::new(editing.draft_name)
                            .a11y_id(name_a11y_id)
                            .auto_focus(true)
                            .flat()
                            .compact()
                            .placeholder("Space Name")
                            .width(Size::fill())
                            .background(Color::TRANSPARENT)
                            .focus_background(Color::TRANSPARENT)
                            .border_fill(Color::TRANSPARENT)
                            .focus_border_fill(Color::TRANSPARENT)
                            .color(palette.text)
                            .placeholder_color(palette.text_muted)
                            .padding(Gaps::new(0.0, 2.0, 0.0, 2.0))
                            .corner_radius(CornerRadius::new_all(0.0))
                            .on_pre_key_down(on_input_key),
                    ),
            );

        let profile_row = rect()
            .width(Size::fill())
            .height(Size::px(38.0))
            .padding(Gaps::new(4.0, 4.0, 4.0, 10.0))
            .direction(Direction::Horizontal)
            .main_align(Alignment::SpaceBetween)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(8.0))
            .background(field_bg)
            .child(
                label()
                    .text("Profile")
                    .font_size(12.5)
                    .font_weight(FontWeight::MEDIUM)
                    .color(palette.text),
            )
            .child(
                rect()
                    .a11y_focusable(true)
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt(format!("Select profile: {current_profile}"))
                    .height(Size::px(28.0))
                    .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
                    .corner_radius(CornerRadius::new_all(6.0))
                    .background(if profile_btn_hovered() { field_hover_bg } else { pill_bg })
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(move |_| profile_btn_hovered.set_if_modified(true))
                    .on_pointer_leave(move |_| profile_btn_hovered.set_if_modified(false))
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        let mut open = editing.profile_menu_open;
                        let next = !*open.peek();
                        open.set(next);
                        let mut icon_p = editing.icon_picker_open;
                        icon_p.set_if_modified(false);
                    })
                    .child(
                        label()
                            .text(current_profile.clone())
                            .font_size(12.0)
                            .font_weight(FontWeight::MEDIUM)
                            .color(palette.text)
                            .max_lines(1),
                    ),
            );

        let profile_dropdown = profile_menu_open.then(|| {
            let menu_bg = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(30, 31, 38, 0.98).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(250, 250, 252, 0.98).to_color(),
            };
            rect()
                .width(Size::fill())
                .padding(Gaps::new_all(4.0))
                .spacing(2.0)
                .corner_radius(CornerRadius::new_all(8.0))
                .background(menu_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.color(ThemeRole::BorderSubtle)),
                )
                .children(SPACE_PROFILES.iter().map(|profile_name| {
                    ProfileOptionRow {
                        name: (*profile_name).to_string(),
                        selected: current_profile == *profile_name,
                        state: editing,
                        key: DiffKey::from(profile_name),
                    }
                    .into_element()
                }))
        });

        let theme_button = rect()
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Edit space theme gradient")
            .width(Size::fill())
            .height(Size::px(38.0))
            .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
            .direction(Direction::Horizontal)
            .main_align(Alignment::center())
            .cross_align(Alignment::center())
            .spacing(8.0)
            .corner_radius(CornerRadius::new_all(8.0))
            .background(if theme_btn_hovered() { field_hover_bg } else { field_bg })
            .cursor(CursorIcon::Pointer)
            .on_sized(move |e: Event<SizedEventData>| {
                let next = (e.area.max_x() + 10.0, (e.area.min_y() - 140.0).max(24.0));
                if *theme_btn_area.peek() != next {
                    theme_btn_area.set(next);
                }
            })
            .on_pointer_enter(move |_| theme_btn_hovered.set_if_modified(true))
            .on_pointer_leave(move |_| theme_btn_hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                editing.open_theme_picker_for_draft(Some(*theme_btn_area.peek()));
            })
            .maybe_child((!draft_dots.is_empty()).then(|| {
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(3.0)
                    .children(draft_dots.iter().enumerate().map(|(i, dot)| {
                        rect()
                            .key(i)
                            .width(Size::px(10.0))
                            .height(Size::px(10.0))
                            .corner_radius(CornerRadius::new_all(999.0))
                            .background(Color::from_rgb(dot.rgb.0, dot.rgb.1, dot.rgb.2))
                            .border(
                                Border::new()
                                    .width(1.0)
                                    .alignment(BorderAlignment::Outer)
                                    .fill(Color::from_argb(180, 255, 255, 255)),
                            )
                    }))
            }))
            .child(
                label()
                    .text("Theme...")
                    .font_size(12.5)
                    .font_weight(FontWeight::MEDIUM)
                    .color(palette.text)
                    .max_lines(1),
            );

        let create_bg = if can_create {
            if create_btn_hovered() {
                theme
                    .rgba(ThemeRole::Accent)
                    .mix(Rgba::rgb(255, 255, 255), 0.14)
                    .to_color()
            } else {
                theme.color(ThemeRole::Accent)
            }
        } else {
            primary_disabled_bg
        };
        let create_fg = if can_create { Color::WHITE } else { primary_disabled_fg };

        let btn_submit = submit_action.clone();
        let create_button = rect()
            .a11y_focusable(can_create)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Create Space")
            .width(Size::fill())
            .height(Size::px(34.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(create_bg)
            .center()
            .cursor(if can_create {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            })
            .on_pointer_enter(move |_| create_btn_hovered.set_if_modified(true))
            .on_pointer_leave(move |_| create_btn_hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                if can_create {
                    btn_submit();
                }
            })
            .child(
                label()
                    .text("Create Space")
                    .font_size(12.5)
                    .font_weight(FontWeight::MEDIUM)
                    .color(create_fg)
                    .max_lines(1),
            );

        let btn_cancel = cancel_action.clone();
        let cancel_button = rect()
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Cancel space creation")
            .width(Size::fill())
            .height(Size::px(34.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(if cancel_btn_hovered() { field_hover_bg } else { field_bg })
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| cancel_btn_hovered.set_if_modified(true))
            .on_pointer_leave(move |_| cancel_btn_hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                btn_cancel();
            })
            .child(
                label()
                    .text("Cancel")
                    .font_size(12.5)
                    .font_weight(FontWeight::MEDIUM)
                    .color(palette.text)
                    .max_lines(1),
            );

        let esc_cancel = cancel_action;
        let icon_picker_overlay = icon_picker_open.then(|| DraftIconPickerPopover {
            state: editing,
            current_icon,
            anchor: *editing.icon_picker_anchor.read(),
        });

        rect()
            .a11y_role(AccessibilityRole::Form)
            .a11y_alt("Create a Space")
            .width(Size::fill())
            .height(Size::flex(1.0))
            .padding(Gaps::new(36.0, 5.0, 6.0, 5.0))
            .content(Content::Flex)
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if matches!(e.key, Key::Named(NamedKey::Escape))
                    && !*editing.icon_picker_open.peek()
                    && !*editing.theme_picker_open.peek()
                {
                    e.stop_propagation();
                    esc_cancel();
                }
            })
            .child(
                rect()
                    .width(Size::fill())
                    .cross_align(Alignment::center())
                    .spacing(6.0)
                    .child(
                        label()
                            .text("Create a Space")
                            .font_size(16.5)
                            .font_weight(FontWeight::BOLD)
                            .text_align(TextAlign::Center)
                            .color(palette.text),
                    )
                    .child(
                        label()
                            .text("Spaces are used to organize your tabs and sessions.")
                            .font_size(11.5)
                            .text_align(TextAlign::Center)
                            .color(palette.text_muted),
                    ),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(40.0, 0.0, 0.0, 0.0))
                    .spacing(9.0)
                    .child(name_row)
                    .child(profile_row)
                    .maybe_child(profile_dropdown)
                    .child(theme_button),
            )
            .child(rect().width(Size::fill()).height(Size::flex(1.0)))
            .child(
                rect()
                    .width(Size::fill())
                    .spacing(8.0)
                    .child(create_button)
                    .child(cancel_button),
            )
            .maybe_child(icon_picker_overlay)
    }
}

fn paint_dashed_square(canvas: &Canvas, width: f32, height: f32, color: SkColor) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.1);
    paint.set_color(color);
    if let Some(effect) = PathEffect::dash(&[3.0, 2.5], 0.0) {
        paint.set_path_effect(effect);
    }
    let inset = 1.0;
    let rrect = RRect::new_rect_xy(
        SkRect::from_xywh(
            inset,
            inset,
            (width - inset * 2.0).max(1.0),
            (height - inset * 2.0).max(1.0),
        ),
        6.0,
        6.0,
    );
    canvas.draw_rrect(rrect, &paint);
}

#[derive(Clone, PartialEq)]
struct ProfileOptionRow {
    name: String,
    selected: bool,
    state: SpaceEditingState,
    key: DiffKey,
}

impl KeyExt for ProfileOptionRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ProfileOptionRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let mut hovered = use_state(|| false);
        let editing = self.state;
        let name = self.name.clone();

        let bg = if self.selected {
            theme.color(ThemeRole::Accent).with_a(44)
        } else if hovered() {
            palette.tab_hover_bg
        } else {
            Color::TRANSPARENT
        };

        rect()
            .a11y_role(AccessibilityRole::MenuItem)
            .a11y_alt(format!("Profile {}", self.name))
            .width(Size::fill())
            .height(Size::px(28.0))
            .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
            .direction(Direction::Horizontal)
            .main_align(Alignment::SpaceBetween)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                let mut prof = editing.draft_profile;
                prof.set(name.clone());
                let mut open = editing.profile_menu_open;
                open.set(false);
            })
            .child(
                label()
                    .text(self.name.clone())
                    .font_size(12.0)
                    .font_weight(if self.selected {
                        FontWeight::SEMI_BOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .color(palette.text)
                    .max_lines(1),
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

#[derive(Clone, PartialEq)]
struct DraftIconPickerPopover {
    state: SpaceEditingState,
    current_icon: String,
    anchor: (f32, f32),
}

impl Component for DraftIconPickerPopover {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let editing = self.state;

        let panel_bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(26, 28, 38, 0.98).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(252, 252, 255, 0.98).to_color(),
        };

        let clear_button = rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Clear space icon")
            .padding(Gaps::new(4.0, 10.0, 4.0, 10.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(palette.surface_pill)
            .cursor(CursorIcon::Pointer)
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                let mut icon = editing.draft_icon;
                icon.set(String::new());
                let mut open = editing.icon_picker_open;
                open.set(false);
            })
            .child(
                label()
                    .text("None")
                    .font_size(11.5)
                    .font_weight(FontWeight::MEDIUM)
                    .color(palette.text_muted),
            );

        let mut rows: Vec<Element> = Vec::new();
        for chunk in SELECTABLE_SPACE_ICONS.chunks(6) {
            rows.push(
                rect()
                    .direction(Direction::Horizontal)
                    .spacing(4.0)
                    .children(chunk.iter().map(|icon_name| {
                        DraftIconTile {
                            icon_name: (*icon_name).to_string(),
                            selected: self.current_icon == *icon_name,
                            state: editing,
                            key: DiffKey::from(icon_name),
                        }
                        .into_element()
                    }))
                    .into_element(),
            );
        }

        let root_size = *Platform::get().current_window().root_size.read();
        let max_x = (root_size.width - 236.0).max(8.0);
        let max_y = (root_size.height - 230.0).max(8.0);
        let x = self.anchor.0.clamp(8.0, max_x);
        let y = self.anchor.1.clamp(8.0, max_y);

        rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if matches!(e.key, Key::Named(NamedKey::Escape)) {
                    e.stop_propagation();
                    let mut open = editing.icon_picker_open;
                    open.set(false);
                }
            })
            .child(
                rect()
                    .position(Position::new_global().left(0.0).top(0.0))
                    .width(Size::window_percent(100.0))
                    .height(Size::window_percent(100.0))
                    .on_mouse_down(move |_| {
                        let mut open = editing.icon_picker_open;
                        open.set(false);
                    }),
            )
            .child(
                rect()
                    .position(Position::new_global().left(x).top(y))
                    .width(Size::px(228.0))
                    .padding(Gaps::new_all(12.0))
                    .spacing(10.0)
                    .corner_radius(CornerRadius::new_all(12.0))
                    .background(panel_bg)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.color(ThemeRole::BorderSubtle)),
                    )
                    .shadow(
                        Shadow::new()
                            .x(0.0)
                            .y(10.0)
                            .blur(24.0)
                            .color(Color::from_argb(95, 0, 0, 0)),
                    )
                    .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
                    .child(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .main_align(Alignment::SpaceBetween)
                            .cross_align(Alignment::center())
                            .child(
                                label()
                                    .text("Choose Icon")
                                    .font_size(12.5)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(palette.text),
                            )
                            .child(clear_button),
                    )
                    .child(rect().spacing(4.0).children(rows)),
            )
    }
}

#[derive(Clone, PartialEq)]
struct DraftIconTile {
    icon_name: String,
    selected: bool,
    state: SpaceEditingState,
    key: DiffKey,
}

impl KeyExt for DraftIconTile {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for DraftIconTile {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let mut hovered = use_state(|| false);
        let editing = self.state;
        let icon_name = self.icon_name.clone();

        let bg = if self.selected {
            theme.color(ThemeRole::Accent).with_a(48)
        } else if hovered() {
            palette.surface_pill_hover
        } else {
            Color::TRANSPARENT
        };

        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Icon {}", self.icon_name))
            .width(Size::px(30.0))
            .height(Size::px(30.0))
            .corner_radius(CornerRadius::new_all(7.0))
            .background(bg)
            .maybe(self.selected, |r| {
                r.border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.color(ThemeRole::Accent)),
                )
            })
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                let mut icon = editing.draft_icon;
                icon.set(icon_name.clone());
                let mut open = editing.icon_picker_open;
                open.set(false);
            })
            .child(
                Icon::new(self.icon_name.clone())
                    .size(16.0)
                    .role(if self.selected || hovered() {
                        ThemeRole::Text
                    } else {
                        ThemeRole::TextMuted
                    }),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
