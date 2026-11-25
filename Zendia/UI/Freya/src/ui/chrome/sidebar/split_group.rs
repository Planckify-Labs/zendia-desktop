/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! A split group in the sidebar: one row holding a chip per pane, as Zen draws it.

use crate::ui::chrome::layout::ChromePalette;
use crate::ui::chrome::menus::{open_tab_context_menu, try_use_menu_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::theme::{ThemeRole, use_theme};
use crate::ui::vim::explorer::{apply_explorer_background, apply_explorer_border};
use crate::ui::vim::hints::hint_badge_for_row;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::focus::FocusTarget;
use zendia_browser_core::model::{SplitGroup, Tab, TabId};

const ROW_HEIGHT: f32 = 36.0;

#[derive(Clone, PartialEq)]
pub struct SplitGroupRow {
    pub group: SplitGroup,
    /// The group's tabs, in pane order.
    pub tabs: Vec<Tab>,
    pub active_tab: Option<TabId>,
    pub collapsed: bool,
}

impl Component for SplitGroupRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let group_active = self.tabs.iter().any(|tab| Some(tab.id) == self.active_tab);

        let chips = self.tabs.iter().map(|tab| {
            SplitGroupChip {
                tab: tab.clone(),
                active: Some(tab.id) == self.active_tab,
                collapsed: self.collapsed,
            }
            .into_element()
        });

        let base_bg = if group_active {
            palette.tab_selected_bg
        } else {
            palette.tab_hover_bg
        };
        let focus_target = FocusTarget::SplitGroup(self.group.id);
        let (bg, border, hint_badge) = {
            let model = core.model().read();
            let base_border = Border::new().width(0.0).fill(Color::TRANSPARENT);
            (
                apply_explorer_background(base_bg, &model, &focus_target, &theme),
                apply_explorer_border(base_border, &model, &focus_target, &theme),
                hint_badge_for_row(&model, &focus_target, ROW_HEIGHT, &theme),
            )
        };

        let row = rect()
            .padding(Gaps::new_all(3.0))
            .spacing(3.0)
            .corner_radius(CornerRadius::new_all(8.0))
            .background(bg)
            .border(border)
            .a11y_role(AccessibilityRole::Group)
            .a11y_alt("Split view");
        if self.collapsed {
            row.cross_align(Alignment::center())
                .children(chips)
                .maybe_child(hint_badge)
        } else {
            row.width(Size::fill())
                .height(Size::px(ROW_HEIGHT))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .cross_align(Alignment::center())
                .children(chips)
                .maybe_child(hint_badge)
        }
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.group.id.0)
    }
}

#[derive(Clone, PartialEq)]
struct SplitGroupChip {
    tab: Tab,
    active: bool,
    collapsed: bool,
}

impl Component for SplitGroupChip {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let menu_state = try_use_menu_state();
        let mut hovered = use_state(|| false);
        let tab_id = self.tab.id;

        let bg = if self.active {
            theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color()
        } else if *hovered.read() {
            theme.rgba(ThemeRole::Text).with_alpha(0.06).to_color()
        } else {
            Color::TRANSPARENT
        };
        let menu_core = core.clone();
        let chip = rect()
            .height(Size::fill())
            .padding(Gaps::new(0.0, 6.0, 0.0, 6.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(6.0)
            .overflow(Overflow::Clip)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_alt(self.tab.display_title())
            .a11y_focusable(true)
            .on_pointer_enter(move |_| hovered.set(true))
            .on_pointer_leave(move |_| hovered.set(false))
            .on_press(move |_| {
                core.invoke_or_log(Command::TabActivate { tab_id });
            })
            .on_mouse_down(move |e: Event<MouseEventData>| {
                if e.button == Some(MouseButton::Right) {
                    e.stop_propagation();
                    if let Some(menu_state) = &menu_state {
                        let model = menu_core.model().peek().clone();
                        let position = (e.global_location.x as f32, e.global_location.y as f32);
                        open_tab_context_menu(menu_state, &model, tab_id, position);
                    }
                }
            })
            .child(
                Icon::new(self.tab.favicon.clone().unwrap_or_else(|| "globe".to_string()))
                    .size(14.0)
                    .role(ThemeRole::Text),
            );
        if self.collapsed {
            chip.width(Size::px(32.0))
                .height(Size::px(28.0))
                .main_align(Alignment::center())
        } else {
            chip.width(Size::flex(1.0)).child(
                label()
                    .text(self.tab.display_title())
                    .font_size(12.5)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Clip)
                    .color(theme.color(ThemeRole::Text)),
            )
        }
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.tab.id.0)
    }
}
