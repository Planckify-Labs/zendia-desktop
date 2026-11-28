/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The library's Space routing section: searchable URL routing rules, add/edit form with match type chips and regex
//! validation, destination space picker, reorder/delete actions, empty state, and external-links default footer.

use crate::ui::chrome::layout::ChromePalette;
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::kit::list_nav::scroll_item_into_view;
use crate::ui::library::nav::{LibraryCursorRing, LibraryNavLayout, LibraryNavRow, use_library_nav};
use crate::ui::library::search::LibrarySearchBar;
use crate::ui::library::try_use_library_state;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, Rgba, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::library::LibrarySection;
use zendia_browser_core::model::{Space, SpaceId};
use zendia_browser_core::space_routing::{
    RouteMatchType, RouteTarget, SpaceRouteId, SpaceRouteRule, filter_rules, validate_pattern,
};

/// Width of this section's body next to the rail (Zen's `418px - 110px`).
pub const WIDTH: f32 = 308.0;

const PILL_RADIUS: f32 = 12.0;

/// Draft state for the inline add/edit route form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceRouteDraft {
    /// `None` when adding a new route; `Some` when editing an existing rule.
    pub id: Option<SpaceRouteId>,
    pub match_type: RouteMatchType,
    pub target: RouteTarget,
}

/// Which space-picker button opened the floating destination menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpacePickerKind {
    DraftTarget,
    DefaultExternal,
}

/// Open state for the destination space picker dropdown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpenSpacePicker {
    pub kind: SpacePickerKind,
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub width: f32,
}

/// View-local state for the library's Space routing section.
#[derive(Clone, Copy, PartialEq)]
pub struct LibrarySpaceRoutingViewState {
    pub draft: State<Option<SpaceRouteDraft>>,
    pub pattern: State<String>,
    pub search_query: State<String>,
    pub open_picker: State<Option<OpenSpacePicker>>,
}

impl LibrarySpaceRoutingViewState {
    /// Opens the form to create a new route.
    pub fn start_new(&self) {
        let mut picker = self.open_picker;
        picker.set(None);
        let mut pattern = self.pattern;
        pattern.set(String::new());
        let mut draft = self.draft;
        draft.set(Some(SpaceRouteDraft {
            id: None,
            match_type: RouteMatchType::Contains,
            target: RouteTarget::MostRecentSpace,
        }));
    }

    /// Opens the form to edit an existing `rule`.
    pub fn start_edit(&self, rule: &SpaceRouteRule) {
        let mut picker = self.open_picker;
        picker.set(None);
        let mut pattern = self.pattern;
        pattern.set(rule.pattern.clone());
        let mut draft = self.draft;
        draft.set(Some(SpaceRouteDraft {
            id: Some(rule.id),
            match_type: rule.match_type,
            target: rule.target,
        }));
    }

    /// Opens the form pre-populated with custom draft values (used by gallery states).
    pub fn set_draft(&self, draft_val: SpaceRouteDraft, pattern_val: impl Into<String>) {
        let mut picker = self.open_picker;
        picker.set(None);
        let mut pattern = self.pattern;
        pattern.set(pattern_val.into());
        let mut draft = self.draft;
        draft.set(Some(draft_val));
    }

    /// Closes the add/edit form.
    pub fn close_form(&self) {
        let mut picker = self.open_picker;
        picker.set(None);
        let mut draft = self.draft;
        draft.set(None);
    }

    /// Closes any open space-picker dropdown.
    pub fn close_picker(&self) {
        let mut picker = self.open_picker;
        picker.set(None);
    }

    /// Resets search query, form, and dropdown state.
    pub fn reset(&self) {
        let mut query = self.search_query;
        query.set(String::new());
        let mut pattern = self.pattern;
        pattern.set(String::new());
        let mut draft = self.draft;
        draft.set(None);
        let mut picker = self.open_picker;
        picker.set(None);
    }
}

/// Installs [`LibrarySpaceRoutingViewState`] into Freya context for the current window.
pub fn use_provide_space_routing_view_state() -> LibrarySpaceRoutingViewState {
    let draft = use_state(|| None);
    let pattern = use_state(String::new);
    let search_query = use_state(String::new);
    let open_picker = use_state(|| None);
    use_provide_context(|| LibrarySpaceRoutingViewState {
        draft,
        pattern,
        search_query,
        open_picker,
    })
}

/// Consumes [`LibrarySpaceRoutingViewState`] if installed in this window.
pub fn try_use_space_routing_view_state() -> Option<LibrarySpaceRoutingViewState> {
    try_consume_context::<LibrarySpaceRoutingViewState>()
}

#[derive(Clone, PartialEq)]
pub struct SpaceRoutingSection;

impl Component for SpaceRoutingSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let provided_state = try_use_space_routing_view_state();
        let fallback_draft = use_state(|| None);
        let fallback_pattern = use_state(String::new);
        let fallback_query = use_state(String::new);
        let fallback_picker = use_state(|| None);
        let routing_state = provided_state.unwrap_or(LibrarySpaceRoutingViewState {
            draft: fallback_draft,
            pattern: fallback_pattern,
            search_query: fallback_query,
            open_picker: fallback_picker,
        });
        let library = try_use_library_state();
        let mut scroll_controller = use_scroll_controller(ScrollConfig::default);
        let mut viewport_height = use_state(|| 420.0f32);
        let mut last_scrolled_cursor = use_state(|| None::<String>);
        let mut handled_escape_seq = use_state(|| 0u64);
        let mut handled_activate_seq = use_state(|| library.map(|l| *l.nav.activate_seq.peek()).unwrap_or(0));

        let (spaces, settings) = {
            let model = core.model().read();
            (model.spaces.clone(), model.space_routing.clone())
        };

        let query_text = routing_state.search_query.read().clone();
        let active_draft = routing_state.draft.read().clone();
        let active_picker = *routing_state.open_picker.read();

        if let Some(lib) = library {
            if active_draft.is_none() {
                let mut form_id = lib.nav.form_input_a11y_id;
                form_id.set_if_modified(None);
            }
            let mut subview_open = lib.nav.subview_open;
            subview_open.set_if_modified(active_draft.is_some() || active_picker.is_some());
            let esc_seq = *lib.nav.subview_escape_seq.read();
            if esc_seq != *handled_escape_seq.peek() {
                handled_escape_seq.set(esc_seq);
                if active_picker.is_some() {
                    routing_state.close_picker();
                } else if active_draft.is_some() {
                    routing_state.close_form();
                }
            }
        }

        let filtered_rules: Vec<SpaceRouteRule> = filter_rules(&settings.rules, &spaces, &query_text)
            .into_iter()
            .cloned()
            .collect();

        // Register walkable rows unconditionally at the top of render (`LIB-05`).
        let nav_rows: Vec<LibraryNavRow> = if active_picker.is_some() {
            let mut rows = vec![LibraryNavRow::new("route-picker:recent", None)];
            for space in &spaces {
                rows.push(LibraryNavRow::new(format!("route-picker:space:{}", space.id.0), None));
            }
            rows
        } else if active_draft.is_some() {
            let mut rows: Vec<LibraryNavRow> = RouteMatchType::ALL
                .into_iter()
                .map(|mt| LibraryNavRow::new(format!("route-form:match:{}", mt.id()), None))
                .collect();
            rows.push(LibraryNavRow::new("route-form:pattern", None));
            rows.push(LibraryNavRow::new("route-form:target", None));
            rows.push(LibraryNavRow::new("route-form:cancel", None));
            rows.push(LibraryNavRow::new("route-form:save", None));
            rows
        } else {
            let mut rows: Vec<LibraryNavRow> = filtered_rules
                .iter()
                .map(|rule| {
                    LibraryNavRow::new(
                        format!("rule:{}", rule.id.0),
                        Some(Command::SpaceRoutingStartEdit { rule_id: rule.id }),
                    )
                })
                .collect();
            rows.push(LibraryNavRow::new("routing:default-external", None));
            rows
        };
        let cursor_key = use_library_nav(LibrarySection::SpaceRouting, LibraryNavLayout::List, nav_rows);

        if let Some(lib) = library {
            let act_seq = *lib.nav.activate_seq.read();
            if act_seq != *handled_activate_seq.peek() {
                handled_activate_seq.set(act_seq);
                if let Some(key) = lib.nav.activated_key.peek().clone() {
                    if key == "routing:default-external" {
                        let mut picker = routing_state.open_picker;
                        let anchor = lib
                            .nav
                            .row_areas
                            .peek()
                            .get("routing:default-external")
                            .map(|&(x, y, w, _)| (x, y - 4.0, w.max(220.0)))
                            .unwrap_or((140.0, 520.0, 280.0));
                        picker.set(Some(OpenSpacePicker {
                            kind: SpacePickerKind::DefaultExternal,
                            anchor_x: anchor.0,
                            anchor_y: anchor.1,
                            width: anchor.2,
                        }));
                    } else if let Some(picker_val) = active_picker {
                        let chosen = if key == "route-picker:recent" {
                            Some(RouteTarget::MostRecentSpace)
                        } else {
                            key.strip_prefix("route-picker:space:")
                                .and_then(|s| s.parse::<u64>().ok())
                                .map(|id| RouteTarget::Space(SpaceId(id)))
                        };
                        if let Some(target) = chosen {
                            match picker_val.kind {
                                SpacePickerKind::DraftTarget => {
                                    let mut draft_state = routing_state.draft;
                                    if let Some(d) = draft_state.write().as_mut() {
                                        d.target = target;
                                    }
                                }
                                SpacePickerKind::DefaultExternal => {
                                    core.invoke_or_log(Command::SpaceRoutingSetDefaultExternal { target });
                                }
                            }
                            routing_state.close_picker();
                        }
                    } else if let Some(ref draft_val) = active_draft {
                        if let Some(mt_id) = key.strip_prefix("route-form:match:") {
                            if let Some(mt) = RouteMatchType::ALL.into_iter().find(|m| m.id() == mt_id) {
                                let mut draft_state = routing_state.draft;
                                if let Some(d) = draft_state.write().as_mut() {
                                    d.match_type = mt;
                                }
                            }
                        } else if key == "route-form:pattern" {
                            if let Some(fid) = *lib.nav.form_input_a11y_id.peek() {
                                let mut ins = lib.nav.inserting;
                                ins.set_if_modified(true);
                                fid.request_focus();
                            }
                        } else if key == "route-form:target" {
                            let anchor = lib
                                .nav
                                .row_areas
                                .peek()
                                .get("route-form:target")
                                .map(|&(x, y, w, h)| (x, y + h + 4.0, w.max(220.0)))
                                .unwrap_or((140.0, 320.0, 280.0));
                            let mut picker = routing_state.open_picker;
                            picker.set(Some(OpenSpacePicker {
                                kind: SpacePickerKind::DraftTarget,
                                anchor_x: anchor.0,
                                anchor_y: anchor.1,
                                width: anchor.2,
                            }));
                        } else if key == "route-form:cancel" {
                            routing_state.close_form();
                        } else if key == "route-form:save" {
                            let pat = routing_state.pattern.peek().clone();
                            if validate_pattern(draft_val.match_type, &pat).is_ok() {
                                let ok = match draft_val.id {
                                    Some(rule_id) => core.invoke_or_log(Command::SpaceRoutingUpdate {
                                        rule_id,
                                        match_type: draft_val.match_type,
                                        pattern: pat,
                                        target: draft_val.target,
                                    }),
                                    None => core.invoke_or_log(Command::SpaceRoutingAdd {
                                        match_type: draft_val.match_type,
                                        pattern: pat,
                                        target: draft_val.target,
                                    }),
                                };
                                if ok {
                                    routing_state.close_form();
                                }
                            }
                        }
                    }
                }
            }
        }

        if active_draft.is_none() && *last_scrolled_cursor.peek() != cursor_key {
            last_scrolled_cursor.set(cursor_key.clone());
            if let Some(target_key) = cursor_key.as_deref()
                && let Some(idx) = filtered_rules
                    .iter()
                    .position(|r| format!("rule:{}", r.id.0) == target_key)
            {
                const ROW_H: f32 = 60.0;
                let item_top = 4.0 + idx as f32 * (ROW_H + 2.0);
                let content_height = 4.0 + filtered_rules.len() as f32 * (ROW_H + 2.0) + 10.0;
                let vh = *viewport_height.peek();
                scroll_item_into_view(&mut scroll_controller, item_top, ROW_H, vh, content_height);
            }
        }

        let is_filtered = !query_text.trim().is_empty();
        let total_rules = settings.rules.len();

        let body: Element = if let Some(draft) = active_draft.clone() {
            SpaceRouteForm {
                draft,
                spaces: spaces.clone(),
                cursor_key: cursor_key.clone(),
                routing_state,
            }
            .into_element()
        } else {
            let list_content: Element = if filtered_rules.is_empty() {
                let empty_text = if settings.rules.is_empty() {
                    "Routes send links to the space you keep them in. Add one for a site you always open in the same place."
                } else {
                    "No routes found"
                };
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(36.0, 20.0, 24.0, 20.0))
                    .cross_align(Alignment::Center)
                    .spacing(10.0)
                    .child(Icon::new("link").size(22.0).role(ThemeRole::TextMuted))
                    .child(
                        label()
                            .text(empty_text)
                            .width(Size::fill())
                            .text_align(TextAlign::Center)
                            .font_size(13.0)
                            .color(palette.text_muted),
                    )
                    .into_element()
            } else {
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(4.0, 10.0, 10.0, 10.0))
                    .spacing(2.0)
                    .children(filtered_rules.iter().enumerate().map(|(idx, rule)| {
                        let focused = cursor_key.as_deref() == Some(&format!("rule:{}", rule.id.0));
                        SpaceRouteRow {
                            rule: rule.clone(),
                            index: idx,
                            total: total_rules,
                            can_reorder: !is_filtered && total_rules > 1,
                            spaces: spaces.clone(),
                            focused,
                            routing_state,
                            key: DiffKey::from(&rule.id.0),
                        }
                        .into_element()
                    }))
                    .into_element()
            };

            rect()
                .width(Size::fill())
                .height(Size::fill())
                .content(Content::Flex)
                .child(
                    LibrarySearchBar::new("Search routes", routing_state.search_query)
                        .trailing_action(NewRouteButton { routing_state }),
                )
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::flex(1.0))
                        .on_sized(move |e: Event<SizedEventData>| {
                            let h = e.area.height();
                            if (*viewport_height.peek() - h).abs() > 1.0 {
                                viewport_height.set(h);
                            }
                            if let Some(lib) = library
                                && h > 1.0
                            {
                                lib.nav.set_visible_rows((h / 62.0).floor() as usize);
                            }
                        })
                        .child(
                            ScrollView::new_controlled(scroll_controller)
                                .width(Size::fill())
                                .height(Size::fill())
                                .show_scrollbar(false)
                                .child(list_content),
                        ),
                )
                .child(
                    // Default external links footer.
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(10.0, 14.0, 14.0, 14.0))
                        .spacing(8.0)
                        .child(
                            rect()
                                .width(Size::fill())
                                .height(Size::px(1.0))
                                .background(palette.divider),
                        )
                        .child(
                            label()
                                .text("Default route for external links")
                                .font_size(12.5)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .color(palette.text_muted),
                        )
                        .child(SpaceTargetPickerButton {
                            kind: SpacePickerKind::DefaultExternal,
                            target: settings.default_external,
                            spaces: spaces.clone(),
                            focused: cursor_key.as_deref() == Some("routing:default-external"),
                            row_key: "routing:default-external".to_string(),
                            routing_state,
                        }),
                )
                .into_element()
        };

        let dropdown = active_picker.map(|picker| {
            let current_target = match picker.kind {
                SpacePickerKind::DraftTarget => active_draft
                    .as_ref()
                    .map(|d| d.target)
                    .unwrap_or(RouteTarget::MostRecentSpace),
                SpacePickerKind::DefaultExternal => settings.default_external,
            };
            SpaceTargetDropdown {
                picker,
                current_target,
                spaces: spaces.clone(),
                cursor_key,
                routing_state,
            }
        });

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .child(body)
            .maybe_child(dropdown)
    }
}

#[derive(Clone, PartialEq)]
struct NewRouteButton {
    routing_state: LibrarySpaceRoutingViewState,
}

impl Component for NewRouteButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let mut hovered = use_state(|| false);
        let routing_state = self.routing_state;

        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("New route")
            .height(Size::px(36.0))
            .padding(Gaps::new(0.0, 12.0, 0.0, 12.0))
            .spacing(6.0)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .corner_radius(CornerRadius::new_all(PILL_RADIUS))
            .background(if hovered() {
                palette.surface_pill_hover
            } else {
                palette.surface_pill
            })
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |_| {
                routing_state.start_new();
            })
            .child(Icon::new("plus").size(14.0).role(ThemeRole::Text))
            .child(
                label()
                    .text("New")
                    .font_size(13.0)
                    .font_weight(FontWeight::MEDIUM)
                    .color(palette.text),
            )
    }
}

#[derive(Clone, PartialEq)]
struct SpaceRouteRow {
    rule: SpaceRouteRule,
    index: usize,
    total: usize,
    can_reorder: bool,
    spaces: Vec<Space>,
    focused: bool,
    routing_state: LibrarySpaceRoutingViewState,
    key: DiffKey,
}

impl KeyExt for SpaceRouteRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SpaceRouteRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let mut hovered = use_state(|| false);
        let mut up_hovered = use_state(|| false);
        let mut down_hovered = use_state(|| false);
        let mut del_hovered = use_state(|| false);

        let rule = self.rule.clone();
        let rule_id = rule.id;
        let routing_state = self.routing_state;
        let idx = self.index;
        let total = self.total;

        let target_space = match rule.target {
            RouteTarget::MostRecentSpace => None,
            RouteTarget::Space(sid) => self.spaces.iter().find(|s| s.id == sid),
        };
        let target_name = target_space
            .map(|s| s.name.as_str())
            .unwrap_or("Most recent space")
            .to_string();
        let target_icon = target_space.map(|s| s.icon.clone()).filter(|i| !i.is_empty());

        let edit_rule = rule.clone();
        let on_row_press = move |_| {
            routing_state.start_edit(&edit_rule);
        };

        let up_core = core.clone();
        let up_button = (hovered() && self.can_reorder && idx > 0).then(|| {
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Move route up")
                .width(Size::px(26.0))
                .height(Size::px(26.0))
                .corner_radius(CornerRadius::new_all(8.0))
                .background(if up_hovered() {
                    palette.surface_pill_hover
                } else {
                    Color::TRANSPARENT
                })
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| up_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| up_hovered.set_if_modified(false))
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    up_core.invoke_or_log(Command::SpaceRoutingReorder {
                        rule_id,
                        index: idx.saturating_sub(1),
                    });
                })
                .child(Icon::new("arrow-up").size(13.0).role(ThemeRole::TextMuted))
        });

        let down_core = core.clone();
        let down_button = (hovered() && self.can_reorder && idx + 1 < total).then(|| {
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Move route down")
                .width(Size::px(26.0))
                .height(Size::px(26.0))
                .corner_radius(CornerRadius::new_all(8.0))
                .background(if down_hovered() {
                    palette.surface_pill_hover
                } else {
                    Color::TRANSPARENT
                })
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| down_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| down_hovered.set_if_modified(false))
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    down_core.invoke_or_log(Command::SpaceRoutingReorder {
                        rule_id,
                        index: idx + 1,
                    });
                })
                .child(Icon::new("arrow-down").size(13.0).role(ThemeRole::TextMuted))
        });

        let del_core = core.clone();
        let delete_button = hovered().then(|| {
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Remove route")
                .width(Size::px(28.0))
                .height(Size::px(28.0))
                .corner_radius(CornerRadius::new_all(8.0))
                .background(if del_hovered() {
                    palette.surface_pill_hover
                } else {
                    Color::TRANSPARENT
                })
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| del_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| del_hovered.set_if_modified(false))
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    del_core.invoke_or_log(Command::SpaceRoutingDelete { rule_id });
                })
                .child(Icon::new("trash").size(14.0).role(if del_hovered() {
                    ThemeRole::Danger
                } else {
                    ThemeRole::TextMuted
                }))
        });

        let row = rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!(
                "{} {} opens in {target_name}",
                rule.match_type.label(),
                rule.pattern
            ))
            .width(Size::fill())
            .padding(Gaps::new(10.0, 12.0, 10.0, 12.0))
            .spacing(10.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .corner_radius(CornerRadius::new_all(16.0))
            .background(if hovered() {
                palette.tab_hover_bg
            } else {
                Color::TRANSPARENT
            })
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(on_row_press)
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .spacing(4.0)
                    .overflow(Overflow::Clip)
                    .child(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .content(Content::Flex)
                            .cross_align(Alignment::Center)
                            .spacing(6.0)
                            .child(Icon::new("link").size(15.0).role(ThemeRole::TextMuted))
                            .child(
                                label()
                                    .text(rule.match_type.label())
                                    .font_size(13.0)
                                    .font_weight(FontWeight::BOLD)
                                    .color(palette.text)
                                    .max_lines(1),
                            )
                            .child(
                                label()
                                    .text(rule.pattern.clone())
                                    .width(Size::flex(1.0))
                                    .font_size(13.0)
                                    .color(palette.text)
                                    .max_lines(1)
                                    .text_overflow(TextOverflow::Ellipsis),
                            ),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new(0.0, 0.0, 0.0, 21.0))
                            .direction(Direction::Horizontal)
                            .content(Content::Flex)
                            .cross_align(Alignment::Center)
                            .spacing(6.0)
                            .child(
                                Icon::new("arrow-corner-down-right")
                                    .size(14.0)
                                    .role(ThemeRole::TextMuted),
                            )
                            .maybe_child(target_icon.map(|name| Icon::new(name).size(13.0).role(ThemeRole::TextMuted)))
                            .child(
                                label()
                                    .text(target_name)
                                    .width(Size::flex(1.0))
                                    .font_size(12.5)
                                    .color(palette.text_muted)
                                    .max_lines(1)
                                    .text_overflow(TextOverflow::Ellipsis),
                            ),
                    ),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::Center)
                    .spacing(2.0)
                    .maybe_child(up_button)
                    .maybe_child(down_button)
                    .maybe_child(delete_button),
            );

        LibraryCursorRing::new(self.focused, row)
            .corner_radius(16.0)
            .row_key(format!("rule:{}", rule_id.0))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SpaceRouteForm {
    draft: SpaceRouteDraft,
    spaces: Vec<Space>,
    cursor_key: Option<String>,
    routing_state: LibrarySpaceRoutingViewState,
}

impl Component for SpaceRouteForm {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let library = try_use_library_state();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let mut cancel_hovered = use_state(|| false);
        let mut save_hovered = use_state(|| false);
        let input_a11y_id = use_hook(AccessibilityId::new_unique);
        if let Some(lib) = library {
            let mut form_id = lib.nav.form_input_a11y_id;
            form_id.set_if_modified(Some(input_a11y_id));
        }

        let routing_state = self.routing_state;
        let draft = self.draft.clone();
        let pattern_text = routing_state.pattern.read().clone();
        let validation = validate_pattern(draft.match_type, &pattern_text);
        let is_valid = validation.is_ok();
        let show_invalid = !pattern_text.trim().is_empty() && !is_valid;
        let error_message = validation.err();

        let title = if draft.id.is_some() { "Edit route" } else { "New route" };
        let save_label = if draft.id.is_some() { "Save route" } else { "Add route" };

        let submit_core = core.clone();
        let submit_draft = draft.clone();
        let commit_action = move || {
            let pat = routing_state.pattern.peek().clone();
            if validate_pattern(submit_draft.match_type, &pat).is_err() {
                return;
            }
            let ok = match submit_draft.id {
                Some(rule_id) => submit_core.invoke_or_log(Command::SpaceRoutingUpdate {
                    rule_id,
                    match_type: submit_draft.match_type,
                    pattern: pat,
                    target: submit_draft.target,
                }),
                None => submit_core.invoke_or_log(Command::SpaceRoutingAdd {
                    match_type: submit_draft.match_type,
                    pattern: pat,
                    target: submit_draft.target,
                }),
            };
            if ok {
                routing_state.close_form();
            }
        };

        let commit_for_btn = commit_action.clone();
        let commit_for_enter = commit_action.clone();
        let commit_for_submit = commit_action;

        let input_border = if show_invalid {
            Border::new()
                .width(1.5)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::Danger))
        } else {
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(palette.urlbar_border)
        };

        let save_bg = if !is_valid {
            palette.surface_pill
        } else if save_hovered() {
            theme.color(ThemeRole::Accent)
        } else {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.20).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.16).to_color(),
            }
        };

        let cursor_key = self.cursor_key.clone();
        let vim_enabled = core.model().read().vim_mode_enabled;

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .padding(Gaps::new(14.0, 14.0, 14.0, 14.0))
            .spacing(16.0)
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if !vim_enabled && matches!(e.key, Key::Named(NamedKey::Escape)) {
                    if routing_state.open_picker.peek().is_some() {
                        routing_state.close_picker();
                    } else {
                        routing_state.close_form();
                    }
                }
            })
            .child(
                label()
                    .text(title)
                    .font_size(14.5)
                    .font_weight(FontWeight::BOLD)
                    .color(palette.text),
            )
            .child(
                // Group 1: When the link
                rect()
                    .width(Size::fill())
                    .spacing(8.0)
                    .child(
                        label()
                            .text("When the link")
                            .font_size(13.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(palette.text_muted),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .content(Content::Flex)
                            .spacing(6.0)
                            .children(RouteMatchType::ALL.into_iter().map(|mt| {
                                let rk = format!("route-form:match:{}", mt.id());
                                let focused = cursor_key.as_deref() == Some(rk.as_str());
                                MatchTypeChip {
                                    match_type: mt,
                                    active: draft.match_type == mt,
                                    focused,
                                    row_key: rk,
                                    routing_state,
                                    key: DiffKey::from(&mt.id()),
                                }
                                .into_element()
                            })),
                    )
                    .child(
                        LibraryCursorRing::new(
                            cursor_key.as_deref() == Some("route-form:pattern"),
                            rect()
                                .width(Size::fill())
                                .height(Size::px(36.0))
                                .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
                                .cross_align(Alignment::Center)
                                .corner_radius(CornerRadius::new_all(PILL_RADIUS))
                                .background(palette.surface_pill)
                                .border(input_border)
                                .on_pointer_down(move |_| {
                                    if let Some(lib) = library {
                                        let mut ins = lib.nav.inserting;
                                        ins.set_if_modified(true);
                                    }
                                })
                                .child(
                                    Input::new(routing_state.pattern)
                                        .a11y_id(input_a11y_id)
                                        .auto_focus(true)
                                        .flat()
                                        .compact()
                                        .placeholder(draft.match_type.placeholder())
                                        .width(Size::fill())
                                        .background(Color::TRANSPARENT)
                                        .focus_background(Color::TRANSPARENT)
                                        .border_fill(Color::TRANSPARENT)
                                        .focus_border_fill(Color::TRANSPARENT)
                                        .color(palette.text)
                                        .placeholder_color(palette.text_muted)
                                        .padding(Gaps::new(0.0, 2.0, 0.0, 2.0))
                                        .corner_radius(CornerRadius::new_all(0.0))
                                        .on_pre_key_down(move |e: Event<KeyboardEventData>| {
                                            e.stop_propagation();
                                            match &e.key {
                                                Key::Named(NamedKey::Escape) => {
                                                    e.prevent_default();
                                                    if vim_enabled && let Some(lib) = library {
                                                        lib.nav.focus_list();
                                                    } else {
                                                        routing_state.close_form();
                                                    }
                                                    false
                                                }
                                                Key::Named(NamedKey::Enter) => {
                                                    e.prevent_default();
                                                    commit_for_enter();
                                                    false
                                                }
                                                _ => true,
                                            }
                                        })
                                        .on_submit(move |_| {
                                            commit_for_submit();
                                        }),
                                ),
                        )
                        .corner_radius(PILL_RADIUS)
                        .row_key("route-form:pattern"),
                    )
                    .maybe_child((show_invalid).then(|| {
                        let msg = error_message
                            .as_deref()
                            .and_then(|s| s.lines().last())
                            .unwrap_or("Invalid regular expression");
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::Center)
                            .spacing(6.0)
                            .padding(Gaps::new(0.0, 4.0, 0.0, 4.0))
                            .child(Icon::new("warning").size(13.0).role(ThemeRole::Danger))
                            .child(
                                label()
                                    .text(format!("Invalid regex: {}", msg.trim()))
                                    .width(Size::flex(1.0))
                                    .font_size(11.5)
                                    .color(theme.color(ThemeRole::Danger))
                                    .max_lines(2)
                                    .text_overflow(TextOverflow::Ellipsis),
                            )
                    })),
            )
            .child(
                // Group 2: Open it in
                rect()
                    .width(Size::fill())
                    .spacing(8.0)
                    .child(
                        label()
                            .text("Open it in")
                            .font_size(13.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(palette.text_muted),
                    )
                    .child(SpaceTargetPickerButton {
                        kind: SpacePickerKind::DraftTarget,
                        target: draft.target,
                        spaces: self.spaces.clone(),
                        focused: cursor_key.as_deref() == Some("route-form:target"),
                        row_key: "route-form:target".to_string(),
                        routing_state,
                    }),
            )
            .child(
                // Action buttons: Cancel and Add/Save route
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(6.0, 0.0, 0.0, 0.0))
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::End)
                    .spacing(8.0)
                    .child(
                        rect().child(
                            LibraryCursorRing::new(
                                cursor_key.as_deref() == Some("route-form:cancel"),
                                rect()
                                    .a11y_role(AccessibilityRole::Button)
                                    .a11y_alt("Cancel")
                                    .padding(Gaps::new(8.0, 14.0, 8.0, 14.0))
                                    .corner_radius(CornerRadius::new_all(PILL_RADIUS))
                                    .background(if cancel_hovered() {
                                        palette.surface_pill_hover
                                    } else {
                                        palette.surface_pill
                                    })
                                    .cursor(CursorIcon::Pointer)
                                    .on_pointer_enter(move |_| cancel_hovered.set_if_modified(true))
                                    .on_pointer_leave(move |_| cancel_hovered.set_if_modified(false))
                                    .on_press(move |_| {
                                        routing_state.close_form();
                                    })
                                    .child(
                                        label()
                                            .text("Cancel")
                                            .font_size(13.0)
                                            .font_weight(FontWeight::SEMI_BOLD)
                                            .color(palette.text),
                                    ),
                            )
                            .corner_radius(PILL_RADIUS)
                            .row_key("route-form:cancel"),
                        ),
                    )
                    .child(
                        rect().child(
                            LibraryCursorRing::new(
                                cursor_key.as_deref() == Some("route-form:save"),
                                rect()
                                    .a11y_role(AccessibilityRole::Button)
                                    .a11y_alt(save_label)
                                    .padding(Gaps::new(8.0, 14.0, 8.0, 14.0))
                                    .corner_radius(CornerRadius::new_all(PILL_RADIUS))
                                    .background(save_bg)
                                    .opacity(if is_valid { 1.0 } else { 0.42 })
                                    .cursor(if is_valid {
                                        CursorIcon::Pointer
                                    } else {
                                        CursorIcon::NotAllowed
                                    })
                                    .on_pointer_enter(move |_| save_hovered.set_if_modified(true))
                                    .on_pointer_leave(move |_| save_hovered.set_if_modified(false))
                                    .on_press(move |_| {
                                        if is_valid {
                                            commit_for_btn();
                                        }
                                    })
                                    .child(
                                        label()
                                            .text(save_label)
                                            .font_size(13.0)
                                            .font_weight(FontWeight::SEMI_BOLD)
                                            .color(if save_hovered() && is_valid {
                                                theme.color(ThemeRole::Surface)
                                            } else {
                                                palette.text
                                            }),
                                    ),
                            )
                            .corner_radius(PILL_RADIUS)
                            .row_key("route-form:save"),
                        ),
                    ),
            )
    }
}

#[derive(Clone, PartialEq)]
struct MatchTypeChip {
    match_type: RouteMatchType,
    active: bool,
    focused: bool,
    row_key: String,
    routing_state: LibrarySpaceRoutingViewState,
    key: DiffKey,
}

impl KeyExt for MatchTypeChip {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for MatchTypeChip {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let mut hovered = use_state(|| false);
        let match_type = self.match_type;
        let routing_state = self.routing_state;

        let bg = if self.active {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.22).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.16).to_color(),
            }
        } else if hovered() {
            palette.surface_pill_hover
        } else {
            palette.surface_pill
        };

        let chip = rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(match_type.label())
            .width(Size::fill())
            .height(Size::px(34.0))
            .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
            .corner_radius(CornerRadius::new_all(PILL_RADIUS))
            .background(bg)
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |_| {
                let mut draft = routing_state.draft;
                if let Some(current) = draft.write().as_mut() {
                    current.match_type = match_type;
                }
            })
            .child(
                label()
                    .text(match_type.label())
                    .font_size(12.5)
                    .font_weight(if self.active {
                        FontWeight::SEMI_BOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .color(palette.text)
                    .max_lines(1),
            );

        rect().width(Size::flex(1.0)).child(
            LibraryCursorRing::new(self.focused, chip)
                .corner_radius(PILL_RADIUS)
                .row_key(self.row_key.clone()),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SpaceTargetPickerButton {
    kind: SpacePickerKind,
    target: RouteTarget,
    spaces: Vec<Space>,
    focused: bool,
    row_key: String,
    routing_state: LibrarySpaceRoutingViewState,
}

impl Component for SpaceTargetPickerButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let mut hovered = use_state(|| false);
        let mut area = use_state(|| (140.0f32, 320.0f32, 360.0f32));

        let kind = self.kind;
        let routing_state = self.routing_state;
        let is_open = routing_state.open_picker.read().is_some_and(|p| p.kind == kind);

        let target_space = match self.target {
            RouteTarget::MostRecentSpace => None,
            RouteTarget::Space(sid) => self.spaces.iter().find(|s| s.id == sid),
        };
        let label_text = target_space
            .map(|s| s.name.as_str())
            .unwrap_or("Most recent space")
            .to_string();
        let icon_name = target_space.map(|s| s.icon.clone()).filter(|i| !i.is_empty());

        let btn = rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Target space: {label_text}"))
            .width(Size::fill())
            .height(Size::px(36.0))
            .padding(Gaps::new(0.0, 12.0, 0.0, 12.0))
            .spacing(8.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .corner_radius(CornerRadius::new_all(PILL_RADIUS))
            .background(if is_open || hovered() {
                palette.surface_pill_hover
            } else {
                palette.surface_pill
            })
            .cursor(CursorIcon::Pointer)
            .on_sized(move |e: Event<SizedEventData>| {
                let y = match kind {
                    SpacePickerKind::DraftTarget => e.area.max_y() + 4.0,
                    SpacePickerKind::DefaultExternal => e.area.min_y() - 4.0,
                };
                area.set_if_modified((e.area.min_x(), y, e.area.width().max(220.0)));
            })
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |_| {
                let mut picker = routing_state.open_picker;
                if picker.peek().is_some_and(|p| p.kind == kind) {
                    picker.set(None);
                } else {
                    let (x, y, w) = *area.peek();
                    picker.set(Some(OpenSpacePicker {
                        kind,
                        anchor_x: x,
                        anchor_y: y,
                        width: w,
                    }));
                }
            })
            .maybe_child(icon_name.map(|name| Icon::new(name).size(14.0).role(ThemeRole::TextMuted)))
            .child(
                label()
                    .text(label_text)
                    .width(Size::flex(1.0))
                    .font_size(13.0)
                    .color(palette.text)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .child(Icon::new("arrow-down").size(12.0).role(ThemeRole::TextMuted));

        LibraryCursorRing::new(self.focused, btn)
            .corner_radius(PILL_RADIUS)
            .row_key(self.row_key.clone())
    }
}

#[derive(Clone, PartialEq)]
struct SpaceTargetDropdown {
    picker: OpenSpacePicker,
    current_target: RouteTarget,
    spaces: Vec<Space>,
    cursor_key: Option<String>,
    routing_state: LibrarySpaceRoutingViewState,
}

impl Component for SpaceTargetDropdown {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let routing_state = self.routing_state;
        let picker = self.picker;
        let cursor_key = self.cursor_key.clone();
        let row_count = self.spaces.len() + 1;
        let est_height = row_count as f32 * 34.0 + 12.0;

        let top = match picker.kind {
            SpacePickerKind::DraftTarget => picker.anchor_y,
            SpacePickerKind::DefaultExternal => (picker.anchor_y - est_height).max(12.0),
        };

        let bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(26, 28, 38, 0.97).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(252, 252, 255, 0.98).to_color(),
        };

        let mut options: Vec<(RouteTarget, String, Option<String>, String)> = vec![(
            RouteTarget::MostRecentSpace,
            "Most recent space".to_string(),
            None,
            "route-picker:recent".to_string(),
        )];
        for space in &self.spaces {
            options.push((
                RouteTarget::Space(space.id),
                space.name.clone(),
                (!space.icon.is_empty()).then(|| space.icon.clone()),
                format!("route-picker:space:{}", space.id.0),
            ));
        }

        let card = rect()
            .position(Position::new_global().left(picker.anchor_x).top(top))
            .width(Size::px(picker.width))
            .padding(Gaps::new_all(6.0))
            .spacing(2.0)
            .corner_radius(CornerRadius::new_all(12.0))
            .background(bg)
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
            .children(options.into_iter().map(|(target, name, icon, row_key)| {
                let key_id: u64 = match target {
                    RouteTarget::MostRecentSpace => 0,
                    RouteTarget::Space(SpaceId(id)) => id,
                };
                let focused = cursor_key.as_deref() == Some(row_key.as_str());
                SpaceTargetDropdownRow {
                    kind: picker.kind,
                    target,
                    label: name,
                    icon,
                    selected: self.current_target == target,
                    focused,
                    row_key,
                    routing_state,
                    key: DiffKey::from(&key_id),
                }
                .into_element()
            }));

        rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .child(
                rect()
                    .position(Position::new_global().left(0.0).top(0.0))
                    .width(Size::window_percent(100.0))
                    .height(Size::window_percent(100.0))
                    .on_mouse_down(move |_| {
                        routing_state.close_picker();
                    }),
            )
            .child(card)
    }
}

#[derive(Clone, PartialEq)]
struct SpaceTargetDropdownRow {
    kind: SpacePickerKind,
    target: RouteTarget,
    label: String,
    icon: Option<String>,
    selected: bool,
    focused: bool,
    row_key: String,
    routing_state: LibrarySpaceRoutingViewState,
    key: DiffKey,
}

impl KeyExt for SpaceTargetDropdownRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SpaceTargetDropdownRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let mut hovered = use_state(|| false);
        let kind = self.kind;
        let target = self.target;
        let routing_state = self.routing_state;

        let row = rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label.clone())
            .width(Size::fill())
            .height(Size::px(32.0))
            .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
            .spacing(8.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .corner_radius(CornerRadius::new_all(8.0))
            .background(if hovered() {
                palette.surface_pill_hover
            } else {
                Color::TRANSPARENT
            })
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |_| {
                match kind {
                    SpacePickerKind::DraftTarget => {
                        let mut draft = routing_state.draft;
                        if let Some(current) = draft.write().as_mut() {
                            current.target = target;
                        }
                    }
                    SpacePickerKind::DefaultExternal => {
                        core.invoke_or_log(Command::SpaceRoutingSetDefaultExternal { target });
                    }
                }
                routing_state.close_picker();
            })
            .maybe_child(
                self.icon
                    .as_ref()
                    .map(|name| Icon::new(name.clone()).size(14.0).role(ThemeRole::TextMuted)),
            )
            .child(
                label()
                    .text(self.label.clone())
                    .width(Size::flex(1.0))
                    .font_size(12.5)
                    .font_weight(if self.selected {
                        FontWeight::SEMI_BOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .color(palette.text)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .maybe_child(
                self.selected
                    .then(|| Icon::new("check").size(13.0).role(ThemeRole::Accent)),
            );

        LibraryCursorRing::new(self.focused, row)
            .corner_radius(8.0)
            .row_key(self.row_key.clone())
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
