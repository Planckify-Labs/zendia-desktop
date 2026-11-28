/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The search box, Filter button, and slide-down filter panel at the top of the History, Downloads, and Space routing
//! sections (Zen's `ZenLibrarySearchSection`).
//!
//! `LibrarySearchBar::new(placeholder, query)` works on its own with no filters; sections with filter groups attach
//! them with [`LibrarySearchBar::filter_title`], [`LibrarySearchBar::filter_groups`],
//! [`LibrarySearchBar::filters_open`], and [`LibrarySearchBar::on_toggle_filter`].

use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::library::nav::{LibraryCursorRing, LibraryNavRow, NavMove};
use crate::ui::library::try_use_library_state;
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use zendia_browser_core::commands::Command;

/// Corner radius shared by the search box, Filter button, Done button, and filter chips (`--zen-library-radius`).
pub const SEARCH_RADIUS: f32 = 12.0;
/// Height of the search box and Filter button.
pub const SEARCH_BOX_HEIGHT: f32 = 36.0;

/// One selectable option chip inside a [`LibraryFilterGroup`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryFilterChip {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub active: bool,
    pub disabled: bool,
}

impl LibraryFilterChip {
    pub fn new(id: impl Into<String>, label: impl Into<String>, active: bool) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            active,
            disabled: false,
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// One titled group of chips in the slide-down filter panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryFilterGroup {
    pub id: String,
    pub title: String,
    pub options: Vec<LibraryFilterChip>,
}

impl LibraryFilterGroup {
    pub fn new(id: impl Into<String>, title: impl Into<String>, options: Vec<LibraryFilterChip>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            options,
        }
    }
}

/// Shared search box and optional Filter button / slide-down filter panel for library sections.
#[derive(Clone, PartialEq)]
pub struct LibrarySearchBar {
    pub placeholder: String,
    pub query: State<String>,
    filter_title: Option<String>,
    filter_groups: Vec<LibraryFilterGroup>,
    filters_open: Option<State<bool>>,
    active_filter_count: usize,
    on_toggle_filter: Option<EventHandler<(String, String)>>,
    trailing_action: Option<Element>,
    auto_focus: bool,
    force_focused_ring: bool,
}

impl LibrarySearchBar {
    pub fn new(placeholder: impl Into<String>, query: State<String>) -> Self {
        Self {
            placeholder: placeholder.into(),
            query,
            filter_title: None,
            filter_groups: Vec::new(),
            filters_open: None,
            active_filter_count: 0,
            on_toggle_filter: None,
            trailing_action: None,
            auto_focus: false,
            force_focused_ring: false,
        }
    }

    pub fn filter_title(mut self, title: impl Into<String>) -> Self {
        self.filter_title = Some(title.into());
        self
    }

    pub fn filter_groups(mut self, groups: Vec<LibraryFilterGroup>) -> Self {
        self.filter_groups = groups;
        self
    }

    pub fn filters_open(mut self, open: State<bool>) -> Self {
        self.filters_open = Some(open);
        self
    }

    /// Shows an active filter count badge on the Filter button when greater than zero.
    pub fn active_filter_count(mut self, count: usize) -> Self {
        self.active_filter_count = count;
        self
    }

    pub fn on_toggle_filter(mut self, handler: impl Into<EventHandler<(String, String)>>) -> Self {
        self.on_toggle_filter = Some(handler.into());
        self
    }

    /// Optional button placed inline on the right of the search input (e.g. Space Routing's `+ New`).
    pub fn trailing_action(mut self, action: impl IntoElement) -> Self {
        self.trailing_action = Some(action.into_element());
        self
    }

    pub fn auto_focus(mut self, auto_focus: bool) -> Self {
        self.auto_focus = auto_focus;
        self
    }

    /// Forces the teal vim focus ring on the search box (used by the gallery's vim-focus state).
    pub fn force_focused_ring(mut self, focused: bool) -> Self {
        self.force_focused_ring = focused;
        self
    }

    /// Builds walkable cursor rows (`filter:done` + `filter:{group}:{chip}`) for an open filter panel (`LIB-05`).
    pub fn filter_nav_rows(groups: &[LibraryFilterGroup]) -> Vec<LibraryNavRow> {
        let mut rows = vec![LibraryNavRow::new("filter:done", None)];
        for group in groups {
            for option in &group.options {
                if !option.disabled {
                    rows.push(LibraryNavRow::new(format!("filter:{}:{}", group.id, option.id), None));
                }
            }
        }
        rows
    }
}

fn pill_colors(scheme: ResolvedScheme) -> (Color, Color, Color) {
    match scheme {
        ResolvedScheme::Dark => (
            Rgba::from_rgb_alpha(255, 255, 255, 0.07).to_color(),
            Rgba::from_rgb_alpha(255, 255, 255, 0.12).to_color(),
            Rgba::from_rgb_alpha(255, 255, 255, 0.22).to_color(),
        ),
        ResolvedScheme::Light => (
            Rgba::from_rgb_alpha(0, 0, 0, 0.06).to_color(),
            Rgba::from_rgb_alpha(0, 0, 0, 0.11).to_color(),
            Rgba::from_rgb_alpha(0, 0, 0, 0.18).to_color(),
        ),
    }
}

impl Component for LibrarySearchBar {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let library = try_use_library_state();
        let input_a11y_id = use_hook(AccessibilityId::new_unique);
        let fallback_filters_open = use_state(|| false);
        let mut filter_btn_hovered = use_state(|| false);
        let mut done_btn_hovered = use_state(|| false);
        let mut last_query = use_state(|| self.query.peek().clone());
        let mut handled_activate_seq = use_state(|| library.map(|l| *l.nav.activate_seq.peek()).unwrap_or(0));
        let mut last_open_focus_key =
            use_state(|| (false, None::<zendia_browser_core::library::LibrarySection>, false));

        let drop_filters_state = self.filters_open.unwrap_or(fallback_filters_open);
        use_drop(move || {
            if let Some(lib) = library {
                let mut nav_fp = lib.nav.filter_panel_open;
                if *nav_fp.peek() == Some(drop_filters_state) {
                    nav_fp.set(None);
                }
                let mut nav_sid = lib.nav.search_a11y_id;
                if *nav_sid.peek() == Some(input_a11y_id) {
                    nav_sid.set(None);
                }
                let mut ins = lib.nav.inserting;
                ins.set_if_modified(false);
            }
        });

        // Reset list cursor to the top match whenever the search query changes (`LIB-KB-02`).
        let current_query = self.query.read().clone();
        if *last_query.peek() != current_query {
            last_query.set(current_query);
            if let Some(lib) = library {
                let mut cursor = lib.nav.cursor;
                cursor.set_if_modified(None);
            }
        }

        let filters_open_state = self.filters_open.unwrap_or(fallback_filters_open);
        let has_filters = !self.filter_groups.is_empty();
        let is_filters_open = has_filters && *filters_open_state.read();
        let cursor_key = library.and_then(|lib| lib.nav.cursor.read().clone());

        // Register this section's search input and filter panel with the keyboard layer, and auto-focus search on open (`LIB-07`).
        if let Some(lib) = library {
            let mut nav_search_id = lib.nav.search_a11y_id;
            nav_search_id.set_if_modified(Some(input_a11y_id));
            let mut nav_filter_panel = lib.nav.filter_panel_open;
            let next_filter_panel = has_filters.then_some(filters_open_state);
            if *nav_filter_panel.peek() != next_filter_panel {
                nav_filter_panel.set(next_filter_panel);
            }

            let is_open = *lib.open.read();
            let active_sec = Some(*lib.section.read());
            let focus_key = (is_open, active_sec, is_filters_open);
            if *last_open_focus_key.peek() != focus_key {
                last_open_focus_key.set(focus_key);
                if is_open && !is_filters_open && *lib.nav.keyboard_focused.peek() {
                    if *lib.nav.stay_in_list.peek() {
                        let mut stay = lib.nav.stay_in_list;
                        stay.set(false);
                        lib.nav.focus_list();
                    } else if self.auto_focus {
                        let mut ins = lib.nav.inserting;
                        ins.set_if_modified(true);
                        input_a11y_id.request_focus();
                    } else {
                        lib.nav.focus_list();
                    }
                }
            }

            // Handle `<CR>` / `o` activation on filter panel controls (`LIB-05`).
            let act_seq = *lib.nav.activate_seq.read();
            if act_seq != *handled_activate_seq.peek() {
                handled_activate_seq.set(act_seq);
                if is_filters_open && let Some(key) = lib.nav.activated_key.peek().clone() {
                    if key == "filter:done" {
                        let mut open_state = filters_open_state;
                        open_state.set(false);
                    } else if let Some(rest) = key.strip_prefix("filter:")
                        && let Some((gid, cid)) = rest.split_once(':')
                        && let Some(handler) = &self.on_toggle_filter
                    {
                        handler.call((gid.to_string(), cid.to_string()));
                    }
                }
            }
        }

        let panel_anim = use_animation_with_dependencies(&is_filters_open, move |conf, open| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *open { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::LIBRARY_FILTER_PANEL)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let panel_progress = panel_anim.get().value().clamp(0.0, 1.0);

        let (pill_bg, pill_hover_bg, pill_active_bg) = pill_colors(theme.scheme);
        let text_color = theme.color(ThemeRole::Text);
        let placeholder_color = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.50).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(24, 28, 42, 0.52).to_color(),
        };
        let vim_enabled = core.model().read().vim_mode_enabled;
        let show_focus_ring = self.force_focused_ring || (vim_enabled && input_a11y_id.is_focused());
        let search_border = if show_focus_ring {
            theme.color(ThemeRole::ModeLibrary)
        } else {
            Color::TRANSPARENT
        };

        let on_pre_key_down = {
            let core = core.clone();
            move |e: Event<KeyboardEventData>| {
                let ctrl = e.modifiers.contains(Modifiers::CONTROL);
                let alt_or_meta = e.modifiers.contains(Modifiers::ALT) || e.modifiers.contains(Modifiers::META);
                let shift = e.modifiers.contains(Modifiers::SHIFT);

                // Let global Alt/Super shortcuts (e.g. Shift+Alt+L) propagate out.
                if alt_or_meta {
                    return true;
                }

                // Ctrl+J / Ctrl+N move down; Ctrl+K / Ctrl+P move up while the search box keeps focus.
                if ctrl && !shift {
                    if let Key::Character(ch) = &e.key {
                        let lower = ch.to_ascii_lowercase();
                        if lower == "j" || lower == "n" || e.code == Code::KeyJ || e.code == Code::KeyN {
                            e.stop_propagation();
                            e.prevent_default();
                            if let Some(lib) = library {
                                lib.nav.move_cursor(NavMove::Down);
                            }
                            return false;
                        }
                        if lower == "k" || lower == "p" || e.code == Code::KeyK || e.code == Code::KeyP {
                            e.stop_propagation();
                            e.prevent_default();
                            if let Some(lib) = library {
                                lib.nav.move_cursor(NavMove::Up);
                            }
                            return false;
                        }
                    }
                    return true;
                }

                match &e.key {
                    Key::Named(NamedKey::ArrowDown) => {
                        e.stop_propagation();
                        e.prevent_default();
                        if let Some(lib) = library {
                            lib.nav.move_cursor(NavMove::Down);
                        }
                        false
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        e.stop_propagation();
                        e.prevent_default();
                        if let Some(lib) = library {
                            lib.nav.move_cursor(NavMove::Up);
                        }
                        false
                    }
                    Key::Named(NamedKey::PageDown) => {
                        e.stop_propagation();
                        e.prevent_default();
                        if let Some(lib) = library {
                            lib.nav.move_cursor(NavMove::HalfPageDown);
                        }
                        false
                    }
                    Key::Named(NamedKey::PageUp) => {
                        e.stop_propagation();
                        e.prevent_default();
                        if let Some(lib) = library {
                            lib.nav.move_cursor(NavMove::HalfPageUp);
                        }
                        false
                    }
                    Key::Named(NamedKey::Enter) => {
                        e.stop_propagation();
                        e.prevent_default();
                        if let Some(lib) = library {
                            lib.nav.activate_cursor(&core);
                        }
                        false
                    }
                    Key::Named(NamedKey::Tab) => {
                        e.stop_propagation();
                        e.prevent_default();
                        if let Some(lib) = library
                            && let Some(next) = lib.adjacent_visible_section(if shift { -1 } else { 1 })
                        {
                            let mut stay = lib.nav.stay_in_list;
                            stay.set_if_modified(false);
                            core.invoke_or_log(Command::LibraryOpen { section: Some(next) });
                        }
                        false
                    }
                    Key::Named(NamedKey::Escape) => {
                        e.stop_propagation();
                        e.prevent_default();
                        if let Some(lib) = library {
                            if lib.nav.close_filter_panel_if_open() {
                                return false;
                            }
                            if core.model().peek().vim_mode_enabled {
                                // Snacks.picker behavior: Escape leaves the search box for the list cursor.
                                lib.nav.focus_list();
                            } else {
                                core.invoke_or_log(Command::LibraryClose);
                            }
                        } else {
                            core.invoke_or_log(Command::LibraryClose);
                        }
                        false
                    }
                    _ => {
                        // Plain typing stays inside the search box and never triggers page/vim shortcuts.
                        e.stop_propagation();
                        true
                    }
                }
            }
        };

        let header: Element = if is_filters_open {
            let mut open_state = filters_open_state;
            let done_focused = cursor_key.as_deref() == Some("filter:done");
            let done_bg = if *done_btn_hovered.read() {
                pill_hover_bg
            } else {
                pill_bg
            };
            let done_btn = rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Done")
                .height(Size::px(34.0))
                .padding(Gaps::new(0.0, 14.0, 0.0, 14.0))
                .corner_radius(CornerRadius::new_all(SEARCH_RADIUS))
                .background(done_bg)
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| done_btn_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| done_btn_hovered.set_if_modified(false))
                .on_press(move |_| open_state.set(false))
                .child(
                    label()
                        .text("Done")
                        .font_size(13.5)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(text_color),
                );
            rect()
                .width(Size::fill())
                .height(Size::px(SEARCH_BOX_HEIGHT))
                .direction(Direction::Horizontal)
                .main_align(Alignment::SpaceBetween)
                .cross_align(Alignment::center())
                .spacing(8.0)
                .child(
                    label()
                        .text(self.filter_title.clone().unwrap_or_else(|| "Filter…".to_string()))
                        .font_size(14.0)
                        .font_weight(FontWeight::BOLD)
                        .color(text_color),
                )
                .child(
                    rect().child(
                        LibraryCursorRing::new(done_focused, done_btn)
                            .corner_radius(SEARCH_RADIUS)
                            .row_key("filter:done"),
                    ),
                )
                .into_element()
        } else {
            let active_count = self.active_filter_count;
            let filter_button = has_filters.then(|| {
                let mut open_state = filters_open_state;
                let bg = if active_count > 0 {
                    theme.rgba(ThemeRole::Accent).with_alpha(0.24).to_color()
                } else if *filter_btn_hovered.read() {
                    pill_hover_bg
                } else {
                    pill_bg
                };
                let label_text = if active_count > 0 {
                    format!("Filter ({active_count})")
                } else {
                    "Filter".to_string()
                };
                let role = if active_count > 0 {
                    ThemeRole::Accent
                } else {
                    ThemeRole::Text
                };
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("Filter")
                    .height(Size::px(SEARCH_BOX_HEIGHT))
                    .padding(Gaps::new(0.0, 12.0, 0.0, 12.0))
                    .corner_radius(CornerRadius::new_all(SEARCH_RADIUS))
                    .background(bg)
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(6.0)
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(move |_| filter_btn_hovered.set_if_modified(true))
                    .on_pointer_leave(move |_| filter_btn_hovered.set_if_modified(false))
                    .on_press(move |_| open_state.set(true))
                    .child(Icon::new("circle-bars-filter").size(15.0).role(role))
                    .child(
                        label()
                            .text(label_text)
                            .font_size(13.5)
                            .font_weight(FontWeight::MEDIUM)
                            .color(theme.color(role)),
                    )
            });

            let submit_core = core.clone();
            rect()
                .width(Size::fill())
                .height(Size::px(SEARCH_BOX_HEIGHT))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .cross_align(Alignment::center())
                .spacing(8.0)
                .child(
                    rect()
                        .width(Size::flex(1.0))
                        .height(Size::px(SEARCH_BOX_HEIGHT))
                        .padding(Gaps::new(0.0, 10.0, 0.0, 12.0))
                        .corner_radius(CornerRadius::new_all(SEARCH_RADIUS))
                        .background(pill_bg)
                        .border(
                            Border::new()
                                .width(1.5)
                                .alignment(BorderAlignment::Inner)
                                .fill(search_border),
                        )
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .cross_align(Alignment::center())
                        .spacing(8.0)
                        .child(
                            rect()
                                .opacity(0.55)
                                .child(Icon::new("search-glass").size(15.0).role(ThemeRole::Text)),
                        )
                        .child(
                            rect()
                                .width(Size::flex(1.0))
                                .font_size(13.5)
                                .on_pointer_down(move |_| {
                                    if let Some(lib) = library {
                                        let mut ins = lib.nav.inserting;
                                        ins.set_if_modified(true);
                                    }
                                })
                                .child(
                                    Input::new(self.query)
                                        .a11y_id(input_a11y_id)
                                        .auto_focus(self.auto_focus)
                                        .flat()
                                        .compact()
                                        .placeholder(self.placeholder.clone())
                                        .width(Size::fill())
                                        .background(Color::TRANSPARENT)
                                        .focus_background(Color::TRANSPARENT)
                                        .border_fill(Color::TRANSPARENT)
                                        .focus_border_fill(Color::TRANSPARENT)
                                        .color(text_color)
                                        .placeholder_color(placeholder_color)
                                        .padding(Gaps::new(0.0, 2.0, 0.0, 2.0))
                                        .corner_radius(CornerRadius::new_all(0.0))
                                        .on_pre_key_down(on_pre_key_down)
                                        .on_submit(move |_: String| {
                                            if let Some(lib) = library {
                                                lib.nav.activate_cursor(&submit_core);
                                            }
                                        }),
                                ),
                        ),
                )
                .maybe_child(filter_button)
                .maybe_child(self.trailing_action.clone())
                .into_element()
        };

        let filter_panel = (has_filters && panel_progress > 0.01).then(|| {
            let mut panel = rect()
                .width(Size::fill())
                .padding(Gaps::new(4.0, 0.0, 6.0, 0.0))
                .opacity(panel_progress)
                .offset_y(-6.0 * (1.0 - panel_progress));

            for group in &self.filter_groups {
                let mut rows_col = rect().width(Size::fill()).spacing(8.0);
                for (row_idx, row_chips) in pack_chip_rows(&group.options, 272.0).into_iter().enumerate() {
                    let mut chips_row = rect()
                        .key(row_idx)
                        .width(Size::fill())
                        .direction(Direction::Horizontal)
                        .spacing(8.0);
                    for option in row_chips {
                        let chip_row_key = format!("filter:{}:{}", group.id, option.id);
                        let chip_focused = cursor_key.as_deref() == Some(chip_row_key.as_str());
                        chips_row = chips_row.child(FilterChipButton {
                            group_id: group.id.clone(),
                            chip: option.clone(),
                            focused: chip_focused,
                            row_key: chip_row_key,
                            pill_bg,
                            pill_hover_bg,
                            pill_active_bg,
                            on_toggle: self.on_toggle_filter.clone(),
                            key: DiffKey::from(&format!("{}:{}", group.id, option.id)),
                        });
                    }
                    rows_col = rows_col.child(chips_row);
                }

                panel = panel.child(
                    rect()
                        .key(group.id.clone())
                        .width(Size::fill())
                        .padding(Gaps::new(12.0, 0.0, 4.0, 0.0))
                        .spacing(8.0)
                        .child(
                            label()
                                .text(group.title.clone())
                                .font_size(12.5)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .color(text_color)
                                .opacity(0.85),
                        )
                        .child(rows_col),
                );
            }
            panel
        });

        rect()
            .width(Size::fill())
            .padding(Gaps::new(12.0, 14.0, 8.0, 14.0))
            .child(header)
            .maybe_child(filter_panel)
    }
}

#[derive(Clone, PartialEq)]
struct FilterChipButton {
    group_id: String,
    chip: LibraryFilterChip,
    focused: bool,
    row_key: String,
    pill_bg: Color,
    pill_hover_bg: Color,
    pill_active_bg: Color,
    on_toggle: Option<EventHandler<(String, String)>>,
    key: DiffKey,
}

impl Component for FilterChipButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);

        let bg = if self.chip.active {
            self.pill_active_bg
        } else if *hovered.read() && !self.chip.disabled {
            self.pill_hover_bg
        } else {
            self.pill_bg
        };

        let disabled = self.chip.disabled;
        let on_toggle = self.on_toggle.clone();
        let group_id = self.group_id.clone();
        let option_id = self.chip.id.clone();

        let chip_el = rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.chip.label.clone())
            .padding(Gaps::new(8.0, 13.0, 8.0, 13.0))
            .corner_radius(CornerRadius::new_all(SEARCH_RADIUS))
            .background(bg)
            .opacity(if disabled { 0.4 } else { 1.0 })
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(6.0)
            .cursor(if disabled {
                CursorIcon::Default
            } else {
                CursorIcon::Pointer
            })
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |_| {
                if !disabled && let Some(handler) = &on_toggle {
                    handler.call((group_id.clone(), option_id.clone()));
                }
            })
            .maybe_child(
                self.chip
                    .icon
                    .as_ref()
                    .map(|icon| Icon::new(icon.clone()).size(15.0).role(ThemeRole::Text)),
            )
            .child(
                label()
                    .text(self.chip.label.clone())
                    .font_size(13.0)
                    .max_lines(1)
                    .font_weight(if self.chip.active {
                        FontWeight::SEMI_BOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .color(theme.color(ThemeRole::Text)),
            );

        rect().child(
            LibraryCursorRing::new(self.focused && !disabled, chip_el)
                .corner_radius(SEARCH_RADIUS)
                .row_key(self.row_key.clone()),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Packs filter chips into horizontal rows that stay within `max_row_width` so no chip text wraps vertically.
fn pack_chip_rows(chips: &[LibraryFilterChip], max_row_width: f32) -> Vec<Vec<&LibraryFilterChip>> {
    const GAP: f32 = 8.0;
    let mut rows: Vec<Vec<&LibraryFilterChip>> = Vec::new();
    let mut current: Vec<&LibraryFilterChip> = Vec::new();
    let mut used = 0.0_f32;

    for chip in chips {
        let icon_w = if chip.icon.is_some() { 21.0 } else { 0.0 };
        let est_w = (chip.label.chars().count() as f32) * 7.2 + 28.0 + icon_w;
        if !current.is_empty() && used + GAP + est_w > max_row_width {
            rows.push(std::mem::take(&mut current));
            used = 0.0;
        }
        if !current.is_empty() {
            used += GAP;
        }
        used += est_w;
        current.push(chip);
    }
    if !current.is_empty() {
        rows.push(current);
    }
    rows
}
