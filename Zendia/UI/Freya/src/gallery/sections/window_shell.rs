/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Window shell gallery section: every toolbar layout × sidebar side × expanded/collapsed state in light and dark
//! themes, with gray placeholder rectangles where Hub pages go.

use crate::ui::chrome::WindowShell;
use crate::ui::chrome::menus::try_use_menu_state;
use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, KitChip, Pill};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{SIDEBAR_DEFAULT_WIDTH, SidebarSide, ToolbarLayout};
use zendia_module_api::ui::UiTone;

#[derive(Clone, PartialEq)]
pub struct WindowShellSection {
    pub state_id: String,
}

fn state_target(state_id: &str) -> (ToolbarLayout, SidebarSide, bool) {
    match state_id {
        "single-right" => (ToolbarLayout::Single, SidebarSide::Right, true),
        "multiple-left" => (ToolbarLayout::Multiple, SidebarSide::Left, true),
        "multiple-right" => (ToolbarLayout::Multiple, SidebarSide::Right, true),
        "collapsed-left" => (ToolbarLayout::Collapsed, SidebarSide::Left, false),
        "collapsed-right" => (ToolbarLayout::Collapsed, SidebarSide::Right, false),
        _ => (ToolbarLayout::Single, SidebarSide::Left, true),
    }
}

fn gray_page_placeholder(title: &str, url: &str) -> Element {
    let theme = use_theme();
    let page_bg = if theme.scheme.is_dark() {
        Color::from_rgb(46, 49, 60)
    } else {
        Color::from_rgb(218, 221, 228)
    };
    let card_bg = if theme.scheme.is_dark() {
        Color::from_rgb(58, 62, 76)
    } else {
        Color::from_rgb(232, 234, 240)
    };

    rect()
        .width(Size::fill())
        .height(Size::fill())
        .background(page_bg)
        .main_align(Alignment::center())
        .cross_align(Alignment::center())
        .child(
            rect()
                .width(Size::px(360.0))
                .padding(Gaps::new_all(20.0))
                .spacing(8.0)
                .cross_align(Alignment::center())
                .corner_radius(CornerRadius::new_all(theme.radii.card))
                .background(card_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.color(ThemeRole::BorderSubtle)),
                )
                .child(Icon::new("page-portrait").size(22.0).role(ThemeRole::TextMuted))
                .child(
                    label()
                        .text(title.to_string())
                        .font_size(theme.font_sizes.lg)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(theme.color(ThemeRole::Text)),
                )
                .child(
                    label()
                        .text(url.to_string())
                        .font_size(theme.font_sizes.xs)
                        .font_family("monospace")
                        .color(theme.color(ThemeRole::TextMuted)),
                ),
        )
        .into_element()
}

impl Component for WindowShellSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let menu_state = try_use_menu_state();

        let mut applied_state = use_state(String::new);
        if *applied_state.peek() != self.state_id {
            applied_state.set(self.state_id.clone());
            let (layout, side, expanded) = state_target(&self.state_id);
            core.invoke_or_log(Command::LayoutSetToolbarLayout { layout });
            core.invoke_or_log(Command::SidebarSetSide { side });
            core.invoke_or_log(Command::SidebarSetExpanded { expanded });
            core.invoke_or_log(Command::SidebarSetWidth {
                width: SIDEBAR_DEFAULT_WIDTH,
            });
        }

        let (active_title, active_url, layout_snapshot) = {
            let model = core.model().read();
            let active = model.active_tab();
            (
                active
                    .map(|tab| tab.display_title())
                    .unwrap_or_else(|| "New Tab".to_string()),
                active
                    .map(|tab| tab.url.clone())
                    .unwrap_or_else(|| "about:newtab".to_string()),
                model.layout.clone(),
            )
        };

        let mut location = use_state(|| active_url.clone());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut shown_url = use_state(|| active_url.clone());
        if *shown_url.peek() != active_url && !location_a11y_id.is_focused() {
            shown_url.set(active_url.clone());
            location.set(active_url.clone());
        }

        let app_menu_open = menu_state.is_some_and(|s| *s.app_menu_open.read());
        let side_core = core.clone();
        let expand_core = core.clone();
        let reset_width_core = core.clone();
        let next_side = match layout_snapshot.sidebar_side {
            SidebarSide::Left => SidebarSide::Right,
            SidebarSide::Right => SidebarSide::Left,
        };

        let info_strip = rect()
            .width(Size::fill())
            .padding(Gaps::new(6.0, 12.0, 6.0, 12.0))
            .direction(Direction::Horizontal)
            .content(Content::wrap())
            .cross_align(Alignment::center())
            .main_align(Alignment::SpaceBetween)
            .spacing(theme.spacing.sm)
            .background(theme.color(ThemeRole::SurfaceRaised))
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(theme.spacing.sm)
                    .child(Pill::new(format!(
                        "{:?} · {:?} · {} · {:.0}px",
                        layout_snapshot.toolbar_layout,
                        layout_snapshot.sidebar_side,
                        if layout_snapshot.sidebar_expanded {
                            "Expanded"
                        } else {
                            "Collapsed"
                        },
                        layout_snapshot.effective_sidebar_width()
                    )))
                    .child(
                        Pill::new(if app_menu_open {
                            "App Menu: Open (seam)"
                        } else {
                            "App Menu: Closed"
                        })
                        .tone(if app_menu_open { UiTone::Accent } else { UiTone::Neutral }),
                    ),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(theme.spacing.xs)
                    .child(KitChip::new("Toggle Expand").on_press(move |_| {
                        expand_core.invoke_or_log(Command::SidebarToggleExpanded);
                    }))
                    .child(KitChip::new("Flip Side").on_press(move |_| {
                        side_core.invoke_or_log(Command::SidebarSetSide { side: next_side });
                    }))
                    .child(KitChip::new("Reset Width").on_press(move |_| {
                        reset_width_core.invoke_or_log(Command::SidebarSetWidth {
                            width: SIDEBAR_DEFAULT_WIDTH,
                        });
                    })),
            );

        let page_placeholder = gray_page_placeholder(&active_title, &active_url);

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .padding(Gaps::new_all(12.0))
            .spacing(10.0)
            .child(info_strip)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .corner_radius(CornerRadius::new_all(10.0))
                    .overflow(Overflow::Clip)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.color(ThemeRole::Border)),
                    )
                    .child(
                        WindowShell::new(location, location_a11y_id, page_placeholder)
                            .interactive_window_controls(false),
                    ),
            )
    }
}
