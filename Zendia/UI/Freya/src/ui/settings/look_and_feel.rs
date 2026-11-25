/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Look and Feel settings group: Browser Layout preview cards, vertical tabs, compact mode, window dragging, and theme.

use crate::ui::core_view::use_core;
use crate::ui::kit::{KitSlider, Pill};
use crate::ui::settings::controls::{
    SettingsChoiceRow, SettingsToggleRow, section_heading, settings_card, subsection_heading,
};
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{
    CompactHide, SidebarSide, ThemeGlassOverrides, ThemeOverrides, ThemeScheme, ToolbarLayout,
};
use zendia_module_api::ui::UiTone;

#[derive(Clone, PartialEq)]
pub struct LookAndFeelGroup;

impl Component for LookAndFeelGroup {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let (layout, browser_settings, theme_settings) = {
            let model = core.model().read();
            (model.layout.clone(), model.settings.clone(), model.theme.clone())
        };

        let glass_opacity = theme_settings.overrides.glass.opacity.unwrap_or(theme.glass.opacity);
        let glass_blur = theme_settings.overrides.glass.blur.unwrap_or(theme.glass.blur);

        let core_single = core.clone();
        let core_multi = core.clone();
        let core_collapsed = core.clone();
        let core_side = core.clone();
        let core_newtab = core.clone();
        let core_newtab_top = core.clone();
        let core_compact = core.clone();
        let core_compact_hide = core.clone();
        let core_flash = core.clone();
        let core_drag = core.clone();
        let core_motion = core.clone();
        let core_scheme = core.clone();
        let core_opacity = core.clone();
        let core_blur = core.clone();

        let scheme_str = match theme_settings.scheme {
            ThemeScheme::System => "system",
            ThemeScheme::Light => "light",
            ThemeScheme::Dark => "dark",
        };
        let side_str = match layout.sidebar_side {
            SidebarSide::Left => "left",
            SidebarSide::Right => "right",
        };
        let overrides_for_opacity = theme_settings.overrides.clone();
        let overrides_for_blur = theme_settings.overrides.clone();

        rect()
            .width(Size::fill())
            .spacing(22.0)
            .child(section_heading(
                "Sidebar and tabs layout",
                Some("Configure how toolbars, vertical tabs, compact mode, and theme colors appear."),
            ))
            .child(
                rect()
                    .width(Size::fill())
                    .spacing(12.0)
                    .child(subsection_heading(
                        "Browser Layout",
                        Some("Choose the layout that suits you best"),
                    ))
                    .child(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .content(Content::wrap())
                            .spacing(16.0)
                            .child(LayoutPreviewCard {
                                layout: ToolbarLayout::Single,
                                title: "Only Sidebar",
                                selected: layout.toolbar_layout == ToolbarLayout::Single,
                                on_press: EventHandler::new(move |_| {
                                    core_single.invoke_or_log(Command::LayoutSetToolbarLayout {
                                        layout: ToolbarLayout::Single,
                                    });
                                }),
                                key: DiffKey::from(&"layout-single"),
                            })
                            .child(LayoutPreviewCard {
                                layout: ToolbarLayout::Multiple,
                                title: "Sidebar and Top Toolbar",
                                selected: layout.toolbar_layout == ToolbarLayout::Multiple,
                                on_press: EventHandler::new(move |_| {
                                    core_multi.invoke_or_log(Command::LayoutSetToolbarLayout {
                                        layout: ToolbarLayout::Multiple,
                                    });
                                }),
                                key: DiffKey::from(&"layout-multiple"),
                            })
                            .child(LayoutPreviewCard {
                                layout: ToolbarLayout::Collapsed,
                                title: "Collapsed Sidebar",
                                selected: layout.toolbar_layout == ToolbarLayout::Collapsed,
                                on_press: EventHandler::new(move |_| {
                                    core_collapsed.invoke_or_log(Command::LayoutSetToolbarLayout {
                                        layout: ToolbarLayout::Collapsed,
                                    });
                                }),
                                key: DiffKey::from(&"layout-collapsed"),
                            }),
                    ),
            )
            .child(settings_card(vec![
                subsection_heading("Vertical Tabs", Some("Manage your tabs in a vertical layout")),
                SettingsChoiceRow::new(
                    "Sidebar position",
                    side_str,
                    vec![
                        ("left".to_string(), "Left side".to_string()),
                        ("right".to_string(), "Right side".to_string()),
                    ],
                    move |val: String| {
                        let side = if val == "right" {
                            SidebarSide::Right
                        } else {
                            SidebarSide::Left
                        };
                        core_side.invoke_or_log(Command::SidebarSetSide { side });
                    },
                )
                .key("sidebar-side")
                .into_element(),
                SettingsToggleRow::new(
                    "Show New Tab Button on Tab List",
                    browser_settings.show_new_tab_on_tab_list,
                    move |next: bool| {
                        core_newtab.invoke_or_log(Command::SettingsSet {
                            key: "tabs.show_newtab_vertical".to_string(),
                            value: next.to_string(),
                        });
                    },
                )
                .key("show-newtab")
                .into_element(),
                SettingsToggleRow::new(
                    "Move the new tab button to the top",
                    browser_settings.new_tab_button_top,
                    move |next: bool| {
                        core_newtab_top.invoke_or_log(Command::SettingsSet {
                            key: "tabs.newtab_button_top".to_string(),
                            value: next.to_string(),
                        });
                    },
                )
                .indent(true)
                .disabled(!browser_settings.show_new_tab_on_tab_list)
                .key("newtab-top")
                .into_element(),
                SettingsToggleRow::new(
                    "Animate page transition when switching tabs",
                    layout.tab_switch_motion,
                    move |next| {
                        core_motion.invoke_or_log(Command::LayoutSetTabSwitchMotion { enabled: next });
                    },
                )
                .key("tab-motion")
                .into_element(),
            ]))
            .child(section_heading("Theme Settings", None))
            .child(settings_card(vec![
                subsection_heading(
                    "Tokyonight Appearance",
                    Some("Switch between system appearance, Dark (Tokyonight Moon), and Light (Tokyonight Day)."),
                ),
                SettingsChoiceRow::new(
                    "Color scheme",
                    scheme_str,
                    vec![
                        ("system".to_string(), "System".to_string()),
                        ("dark".to_string(), "Dark".to_string()),
                        ("light".to_string(), "Light".to_string()),
                    ],
                    move |val: String| {
                        let scheme = match val.as_str() {
                            "light" => ThemeScheme::Light,
                            "dark" => ThemeScheme::Dark,
                            _ => ThemeScheme::System,
                        };
                        core_scheme.invoke_or_log(Command::ThemeSet {
                            scheme: Some(scheme),
                            overrides: None,
                        });
                    },
                )
                .key("theme-scheme")
                .into_element(),
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(4.0, 8.0, 4.0, 8.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .main_align(Alignment::SpaceBetween)
                    .spacing(16.0)
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .spacing(4.0)
                            .child(
                                label()
                                    .text(format!("Glass opacity · {}%", (glass_opacity * 100.0).round() as i32))
                                    .font_size(12.5)
                                    .color(theme.color(ThemeRole::Text)),
                            )
                            .child(
                                KitSlider::new(glass_opacity)
                                    .range(0.4, 1.0)
                                    .step(Some(0.05))
                                    .width(Size::fill())
                                    .on_change(move |val| {
                                        let mut next: ThemeOverrides = overrides_for_opacity.clone();
                                        next.glass = ThemeGlassOverrides {
                                            opacity: Some(val),
                                            blur: next.glass.blur,
                                        };
                                        core_opacity.invoke_or_log(Command::ThemeSet {
                                            scheme: None,
                                            overrides: Some(next),
                                        });
                                    }),
                            ),
                    )
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .spacing(4.0)
                            .child(
                                label()
                                    .text(format!("Glass blur · {} px", glass_blur.round() as i32))
                                    .font_size(12.5)
                                    .color(theme.color(ThemeRole::Text)),
                            )
                            .child(
                                KitSlider::new(glass_blur)
                                    .range(0.0, 28.0)
                                    .step(Some(2.0))
                                    .width(Size::fill())
                                    .on_change(move |val| {
                                        let mut next: ThemeOverrides = overrides_for_blur.clone();
                                        next.glass = ThemeGlassOverrides {
                                            opacity: next.glass.opacity,
                                            blur: Some(val),
                                        };
                                        core_blur.invoke_or_log(Command::ThemeSet {
                                            scheme: None,
                                            overrides: Some(next),
                                        });
                                    }),
                            ),
                    )
                    .into_element(),
            ]))
            .child(settings_card(vec![
                subsection_heading("Show in compact view", Some("Only show the toolbars you use!")),
                SettingsToggleRow::new("Enable Zendia's compact mode", layout.compact_mode, move |next| {
                    core_compact.invoke_or_log(Command::LayoutSetCompactMode { enabled: next });
                })
                .key("compact-enabled")
                .into_element(),
                SettingsChoiceRow::new(
                    "Compact mode hides",
                    layout.compact_hide.as_str(),
                    vec![
                        ("both".to_string(), "Hide Both".to_string()),
                        ("sidebar".to_string(), "Hide Tab Bar".to_string()),
                        ("toolbar".to_string(), "Hide Top Bar".to_string()),
                    ],
                    move |val: String| {
                        if let Some(hide) = CompactHide::parse(&val) {
                            core_compact_hide.invoke_or_log(Command::LayoutSetCompactHide { hide });
                        }
                    },
                )
                .key("compact-hide")
                .into_element(),
                SettingsToggleRow::new(
                    "Briefly make the toolbar popup when switching or opening new tabs in compact mode",
                    browser_settings.compact_toolbar_flash_popup,
                    move |next: bool| {
                        core_flash.invoke_or_log(Command::SettingsSet {
                            key: "compact.toolbar_flash_popup".to_string(),
                            value: next.to_string(),
                        });
                    },
                )
                .indent(true)
                .key("compact-flash")
                .into_element(),
            ]))
            .child(settings_card(vec![
                subsection_heading(
                    "Window dragging",
                    Some("Move the window by dragging empty space at the top of websites, just like the titlebar."),
                ),
                SettingsToggleRow::new(
                    "Allow dragging the window from web pages",
                    browser_settings.window_drag_from_content,
                    move |next: bool| {
                        core_drag.invoke_or_log(Command::SettingsSet {
                            key: "window.drag_from_content".to_string(),
                            value: next.to_string(),
                        });
                    },
                )
                .key("window-drag")
                .into_element(),
            ]))
    }
}

/// Visual layout picker card matching Zen's three Browser Layout preview tiles.
#[derive(Clone, PartialEq)]
struct LayoutPreviewCard {
    layout: ToolbarLayout,
    title: &'static str,
    selected: bool,
    on_press: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for LayoutPreviewCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for LayoutPreviewCard {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_press = self.on_press.clone();

        let backdrop = LinearGradient::new()
            .angle(135.0)
            .stop((Rgba::rgb(214, 118, 168).to_color(), 0.0))
            .stop((Rgba::rgb(138, 92, 184).to_color(), 52.0))
            .stop((Rgba::rgb(68, 86, 168).to_color(), 100.0));

        let chrome_bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::rgb(34, 37, 54).to_color(),
            ResolvedScheme::Light => Rgba::rgb(236, 238, 246).to_color(),
        };
        let page_bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::rgb(22, 24, 36).to_color(),
            ResolvedScheme::Light => Rgba::rgb(255, 255, 255).to_color(),
        };
        let pill_col = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.16).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(20, 24, 42, 0.12).to_color(),
        };
        let active_pill = theme.rgba(ThemeRole::Accent).with_alpha(0.75).to_color();

        let has_top = !matches!(self.layout, ToolbarLayout::Single);
        let collapsed = matches!(self.layout, ToolbarLayout::Collapsed);
        let sb_width = if collapsed { 24.0 } else { 74.0 };

        let mini_top_bar = has_top.then(|| {
            rect()
                .width(Size::fill())
                .height(Size::px(18.0))
                .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .spacing(6.0)
                .child(
                    rect()
                        .width(Size::px(12.0))
                        .height(Size::px(6.0))
                        .corner_radius(CornerRadius::new_all(3.0))
                        .background(pill_col),
                )
                .child(
                    rect()
                        .width(Size::px(76.0))
                        .height(Size::px(9.0))
                        .corner_radius(CornerRadius::new_all(4.0))
                        .background(pill_col),
                )
        });

        let mini_sidebar = rect()
            .width(Size::px(sb_width))
            .height(Size::fill())
            .padding(Gaps::new(6.0, 5.0, 6.0, 5.0))
            .spacing(5.0)
            .maybe_child((!has_top).then(|| {
                rect()
                    .width(Size::fill())
                    .height(Size::px(11.0))
                    .corner_radius(CornerRadius::new_all(4.0))
                    .background(pill_col)
            }))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(10.0))
                    .corner_radius(CornerRadius::new_all(4.0))
                    .background(active_pill),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(9.0))
                    .corner_radius(CornerRadius::new_all(4.0))
                    .background(pill_col),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(9.0))
                    .corner_radius(CornerRadius::new_all(4.0))
                    .background(pill_col),
            );

        let mini_browser = rect()
            .position(Position::new_absolute().left(32.0).top(24.0))
            .width(Size::px(206.0))
            .height(Size::px(124.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .overflow(Overflow::Clip)
            .background(chrome_bg)
            .shadow(Shadow::new().y(4.0).blur(14.0).color(Color::from_argb(95, 0, 0, 0)))
            .maybe_child(mini_top_bar)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .padding(Gaps::new(if has_top { 0.0 } else { 4.0 }, 5.0, 5.0, 4.0))
                    .spacing(4.0)
                    .child(mini_sidebar)
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .height(Size::fill())
                            .corner_radius(CornerRadius::new_all(5.0))
                            .background(page_bg),
                    ),
            );

        let border_col = if self.selected || focus() == Focus::Keyboard {
            theme.color(ThemeRole::Accent)
        } else if *hovered.read() {
            theme.color(ThemeRole::Border)
        } else {
            theme.color(ThemeRole::BorderSubtle)
        };
        let border_w = if self.selected || focus() == Focus::Keyboard {
            2.5
        } else {
            1.0
        };

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Browser layout: {}", self.title))
            .width(Size::px(236.0))
            .spacing(8.0)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |_| on_press.call(()))
            .child(
                rect()
                    .width(Size::px(236.0))
                    .height(Size::px(140.0))
                    .corner_radius(CornerRadius::new_all(10.0))
                    .overflow(Overflow::Clip)
                    .background(backdrop)
                    .border(
                        Border::new()
                            .width(border_w)
                            .alignment(BorderAlignment::Inner)
                            .fill(border_col),
                    )
                    .child(mini_browser),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::SpaceBetween)
                    .cross_align(Alignment::center())
                    .child(
                        label()
                            .text(self.title)
                            .font_size(13.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(theme.color(ThemeRole::Text)),
                    )
                    .maybe_child(self.selected.then(|| Pill::new("Active").tone(UiTone::Accent))),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
