/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Floating Model & Effort card popover anchored above the agent composer's model button.
//!
//! Matches Zendia's `ZendiaAgentModelCard` layout:
//! - Main view: stacked effort heading (`High`) + clickable muted model link (`Sonnet 4.6 ›`), 28px effort slider
//!   with colored fill, stop dots, and white thumb outlined in the active effort color, plus `Context window` row
//!   (`1M ›`) with expandable choices.
//! - Searchable model list sub-view: single thin `‹` back chevron + compact `Search models…` input, Title Case
//!   grouped sections (`Recommended`, `Fast`, `Other`), accent-tinted active row with `✓`, and bottom detail bar.

use crate::ui::agent::{agent_input_colors, agent_input_layout, chevron_left_icon, chevron_right_icon};
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::panels::try_use_dock_state;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::agent::{format_tokens, search_models};
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{AgentId, ModelEffort, ModelId, ModelInfo};

/// Width of the floating model card in logical pixels.
pub const MODEL_CARD_WIDTH: f32 = 256.0;
const RAIL_WIDTH: f32 = 228.0;
const RAIL_HEIGHT: f32 = 26.0;
const THUMB_WIDTH: f32 = 42.0;
const THUMB_HEIGHT: f32 = 22.0;

fn effort_role(effort: ModelEffort) -> ThemeRole {
    match effort {
        ModelEffort::None => ThemeRole::EffortNone,
        ModelEffort::Minimal => ThemeRole::EffortMinimal,
        ModelEffort::Low => ThemeRole::EffortLow,
        ModelEffort::Medium => ThemeRole::EffortMedium,
        ModelEffort::High => ThemeRole::EffortHigh,
        ModelEffort::XHigh => ThemeRole::EffortXHigh,
        ModelEffort::Max => ThemeRole::EffortMax,
        ModelEffort::Ultra => ThemeRole::EffortUltra,
    }
}

/// Shortens a full model name (`"Claude Sonnet 4.6"` -> `"Sonnet 4.6"`) for compact inline labels.
pub fn short_model_name(name: &str) -> &str {
    name.strip_prefix("Claude ").unwrap_or(name)
}

/// Anchored floating popover for choosing model, reasoning effort, and context window.
#[derive(Clone, PartialEq)]
pub struct AgentModelCard {
    pub agent_id: AgentId,
    pub selected_model: ModelInfo,
    pub effective_effort: ModelEffort,
    pub active_context_id: Option<String>,
    pub models: Vec<ModelInfo>,
    pub accent: Color,
    key: DiffKey,
}

impl KeyExt for AgentModelCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl AgentModelCard {
    pub fn new(
        agent_id: AgentId,
        selected_model: ModelInfo,
        effective_effort: ModelEffort,
        active_context_id: Option<String>,
        models: Vec<ModelInfo>,
        accent: Color,
    ) -> Self {
        Self {
            agent_id,
            selected_model,
            effective_effort,
            active_context_id,
            models,
            accent,
            key: DiffKey::None,
        }
    }
}

impl Component for AgentModelCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let dock_state = try_use_dock_state();
        let search_query = use_state(String::new);
        let mut context_menu_open = use_state(|| false);
        let mut hovered_model = use_state(|| None::<ModelId>);
        let mut rail_area = use_state(|| (0.0f32, RAIL_WIDTH));
        let mut dragging_slider = use_state(|| false);
        let mut last_slider_click_ms = use_state(|| 0u128);

        let is_list_view = dock_state.is_some_and(|ds| *ds.model_card_list_view.read());

        let bg = theme.color(ThemeRole::SurfaceModelGlass);
        let border_col = theme.color(ThemeRole::BorderSubtle);
        let subtle_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(24, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(14, 20, 24, 38),
        };
        let hover_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(28, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(18, 20, 24, 38),
        };

        let content: Element = if is_list_view {
            let query = search_query.read().clone();
            let filtered = search_models(&self.models, &query);
            let hovered_info = hovered_model
                .read()
                .as_ref()
                .and_then(|id| self.models.iter().find(|m| &m.id == id))
                .unwrap_or(&self.selected_model);

            let detail_text = match &hovered_info.detail {
                Some(d) => format!("{} · {d}", hovered_info.id.0),
                None => format!(
                    "{} · {} context",
                    hovered_info.id.0,
                    format_tokens(hovered_info.context_window)
                ),
            };

            let mut list_col = rect().width(Size::fill()).spacing(2.0);
            let mut last_section: Option<String> = None;

            if filtered.is_empty() {
                list_col = list_col.child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(16.0, 8.0, 16.0, 8.0))
                        .center()
                        .child(
                            label()
                                .text("No matching models")
                                .font_size(11.5)
                                .color(theme.color(ThemeRole::TextMuted)),
                        ),
                );
            } else {
                for model_item in filtered {
                    let section = model_item.section.clone().unwrap_or_else(|| "Models".to_string());
                    if last_section.as_deref() != Some(section.as_str()) {
                        last_section = Some(section.clone());
                        list_col = list_col.child(
                            rect().width(Size::fill()).padding(Gaps::new(6.0, 8.0, 2.0, 8.0)).child(
                                label()
                                    .text(section)
                                    .font_size(10.5)
                                    .font_weight(FontWeight::MEDIUM)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            ),
                        );
                    }

                    let is_selected = model_item.id == self.selected_model.id;
                    let is_hovered = hovered_model.read().as_ref() == Some(&model_item.id);
                    let row_bg = if is_selected {
                        theme.rgba(ThemeRole::Accent).with_alpha(0.22).to_color()
                    } else if is_hovered {
                        hover_bg
                    } else {
                        Color::TRANSPARENT
                    };

                    let mid_enter = model_item.id.clone();
                    let mid_leave = model_item.id.clone();
                    let mid_pick = model_item.id.clone();
                    let agent_pick = self.agent_id.clone();
                    let core_pick = core.clone();

                    list_col = list_col.child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt(model_item.name.clone())
                            .width(Size::fill())
                            .height(Size::px(30.0))
                            .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
                            .corner_radius(CornerRadius::new_all(8.0))
                            .background(row_bg)
                            .direction(Direction::Horizontal)
                            .content(Content::Flex)
                            .cross_align(Alignment::center())
                            .main_align(Alignment::SpaceBetween)
                            .spacing(6.0)
                            .cursor(CursorIcon::Pointer)
                            .on_pointer_enter(move |_| hovered_model.set_if_modified(Some(mid_enter.clone())))
                            .on_pointer_leave(move |_| {
                                if hovered_model.peek().as_ref() == Some(&mid_leave) {
                                    hovered_model.set(None);
                                }
                            })
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_pick.invoke_or_log(Command::AgentModel {
                                    agent_id: Some(agent_pick.clone()),
                                    model_id: Some(mid_pick.clone()),
                                });
                                if let Some(mut ds) = dock_state {
                                    ds.model_card_list_view.set(false);
                                }
                            })
                            .child(
                                rect().width(Size::flex(1.0)).child(
                                    label()
                                        .text(model_item.name.clone())
                                        .font_size(12.0)
                                        .font_weight(if is_selected {
                                            FontWeight::SEMI_BOLD
                                        } else {
                                            FontWeight::NORMAL
                                        })
                                        .color(theme.color(ThemeRole::Text))
                                        .max_lines(1),
                                ),
                            )
                            .maybe_child(is_selected.then(|| Icon::new("check").size(12.0).role(ThemeRole::Accent))),
                    );
                }
            }

            rect()
                .width(Size::fill())
                .spacing(4.0)
                .child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(8.0, 10.0, 6.0, 8.0))
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .cross_align(Alignment::center())
                        .spacing(6.0)
                        .child(
                            rect()
                                .a11y_role(AccessibilityRole::Button)
                                .a11y_alt("Back to effort settings")
                                .width(Size::px(22.0))
                                .height(Size::px(22.0))
                                .corner_radius(CornerRadius::new_all(6.0))
                                .center()
                                .cursor(CursorIcon::Pointer)
                                .on_press(move |e: Event<PressEventData>| {
                                    e.stop_propagation();
                                    if let Some(mut ds) = dock_state {
                                        ds.model_card_list_view.set(false);
                                    }
                                })
                                .child(chevron_left_icon(12.0, theme.color(ThemeRole::TextMuted))),
                        )
                        .child(
                            rect().width(Size::flex(1.0)).font_size(12.0).child(
                                Input::new(search_query)
                                    .placeholder("Search models…")
                                    .width(Size::fill())
                                    .compact()
                                    .theme_colors(agent_input_colors(&theme, true))
                                    .theme_layout(agent_input_layout(Gaps::new(3.0, 4.0, 3.0, 4.0), 6.0)),
                            ),
                        ),
                )
                .child(rect().width(Size::fill()).height(Size::px(1.0)).background(border_col))
                .child(
                    ScrollView::new().width(Size::fill()).height(Size::px(164.0)).child(
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new(2.0, 6.0, 4.0, 6.0))
                            .child(list_col),
                    ),
                )
                .child(rect().width(Size::fill()).height(Size::px(1.0)).background(border_col))
                .child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(5.0, 10.0, 7.0, 10.0))
                        .child(
                            label()
                                .text(detail_text)
                                .font_size(10.5)
                                .color(theme.color(ThemeRole::TextMuted))
                                .max_lines(1)
                                .text_overflow(TextOverflow::Ellipsis),
                        ),
                )
                .into_element()
        } else {
            // Main view: Stacked Effort title + clickable model link + colored effort slider + Context window row.
            let efforts = if self.selected_model.efforts.is_empty() {
                vec![ModelEffort::Low, ModelEffort::Medium, ModelEffort::High]
            } else {
                self.selected_model.efforts.clone()
            };
            let active_idx = efforts
                .iter()
                .position(|e| *e == self.effective_effort)
                .unwrap_or_else(|| efforts.len().saturating_sub(1));
            let stops = efforts.len().max(2);
            let frac = active_idx as f32 / (stops - 1) as f32;
            let active_role = effort_role(self.effective_effort);
            let active_color = theme.color(active_role);

            let travel = (RAIL_WIDTH - THUMB_WIDTH - 4.0).max(0.0);
            let thumb_left = 2.0 + travel * frac;
            let fill_w = (thumb_left + THUMB_WIDTH * 0.5).clamp(THUMB_WIDTH * 0.5, RAIL_WIDTH);

            let efforts_down = efforts.clone();
            let efforts_move = efforts.clone();
            let agent_down = self.agent_id.clone();
            let agent_move = self.agent_id.clone();
            let core_down = core.clone();
            let core_move = core.clone();

            let pick_effort_at = move |global_x: f32,
                                       list: &[ModelEffort],
                                       core_ref: &crate::ui::core_view::CoreView,
                                       agent: &AgentId| {
                let (origin_x, width) = *rail_area.peek();
                let w = width.max(40.0);
                let rel = ((global_x - origin_x) / w).clamp(0.0, 1.0);
                let idx = (rel * (list.len().saturating_sub(1)) as f32).round() as usize;
                if let Some(&effort) = list.get(idx) {
                    core_ref.invoke_or_log(Command::AgentSetEffort {
                        agent_id: Some(agent.clone()),
                        effort: Some(effort),
                    });
                }
            };

            let mut dots_layer = rect()
                .position(Position::new_absolute().left(0.0).top(0.0))
                .width(Size::px(RAIL_WIDTH))
                .height(Size::px(RAIL_HEIGHT))
                .padding(Gaps::new(0.0, THUMB_WIDTH * 0.5, 0.0, THUMB_WIDTH * 0.5))
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .main_align(Alignment::SpaceBetween);
            for i in 0..stops {
                let dot_col = if i <= active_idx {
                    Color::from_argb(150, 20, 22, 34)
                } else {
                    theme.rgba(ThemeRole::TextMuted).with_alpha(0.55).to_color()
                };
                dots_layer = dots_layer.child(
                    rect()
                        .width(Size::px(3.5))
                        .height(Size::px(3.5))
                        .corner_radius(CornerRadius::new_all(999.0))
                        .background(dot_col),
                );
            }

            let effort_slider = rect()
                .width(Size::px(RAIL_WIDTH))
                .height(Size::px(RAIL_HEIGHT))
                .corner_radius(CornerRadius::new_all(999.0))
                .overflow(Overflow::Clip)
                .background(subtle_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(border_col),
                )
                .cursor(CursorIcon::Pointer)
                .on_sized(move |e: Event<SizedEventData>| {
                    rail_area.set_if_modified((e.area.min_x(), e.area.width()));
                })
                .on_pointer_down(move |e: Event<PointerEventData>| {
                    e.stop_propagation();
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    let prev = *last_slider_click_ms.peek();
                    last_slider_click_ms.set(now);
                    if now.saturating_sub(prev) < 350 {
                        dragging_slider.set(false);
                        core_down.invoke_or_log(Command::AgentSetEffort {
                            agent_id: Some(agent_down.clone()),
                            effort: None,
                        });
                        return;
                    }
                    dragging_slider.set(true);
                    pick_effort_at(e.global_location().x as f32, &efforts_down, &core_down, &agent_down);
                })
                .on_global_pointer_move(move |e: Event<PointerEventData>| {
                    if *dragging_slider.peek() {
                        pick_effort_at(e.global_location().x as f32, &efforts_move, &core_move, &agent_move);
                    }
                })
                .on_global_pointer_up(move |_| {
                    if *dragging_slider.peek() {
                        dragging_slider.set(false);
                    }
                })
                .child(
                    rect()
                        .position(Position::new_absolute().left(0.0).top(0.0))
                        .width(Size::px(fill_w))
                        .height(Size::px(RAIL_HEIGHT))
                        .corner_radius(CornerRadius::new_all(999.0))
                        .background(active_color),
                )
                .child(dots_layer)
                .child(
                    rect()
                        .position(Position::new_absolute().left(thumb_left).top(2.0))
                        .width(Size::px(THUMB_WIDTH))
                        .height(Size::px(THUMB_HEIGHT))
                        .corner_radius(CornerRadius::new_all(999.0))
                        .background(Color::WHITE)
                        .border(
                            Border::new()
                                .width(1.5)
                                .alignment(BorderAlignment::Inner)
                                .fill(active_color),
                        )
                        .shadow(Shadow::new().y(1.0).blur(4.0).color(Color::from_argb(75, 0, 0, 0))),
                );

            let context_section =
                (!self.selected_model.context_choices.is_empty()).then(|| {
                    let active_ctx = self
                        .active_context_id
                        .as_deref()
                        .and_then(|id| self.selected_model.context_choices.iter().find(|c| c.id == id))
                        .or_else(|| self.selected_model.context_choices.iter().find(|c| c.is_default))
                        .or_else(|| self.selected_model.context_choices.first());
                    let active_label = active_ctx
                        .map(|c| c.label.clone())
                        .unwrap_or_else(|| format_tokens(self.selected_model.context_window));
                    let active_id_owned = active_ctx.map(|c| c.id.clone());
                    let expanded = *context_menu_open.read();

                    let mut choices_row = rect()
                        .width(Size::fill())
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .spacing(6.0)
                        .padding(Gaps::new(2.0, 0.0, 2.0, 0.0));
                    for choice in &self.selected_model.context_choices {
                        let is_cur = active_id_owned.as_deref() == Some(choice.id.as_str());
                        let cid = choice.id.clone();
                        let aid = self.agent_id.clone();
                        let core_ctx = core.clone();
                        choices_row =
                            choices_row.child(
                                rect()
                                    .a11y_role(AccessibilityRole::Button)
                                    .a11y_alt(format!("Set context window to {}", choice.label))
                                    .width(Size::flex(1.0))
                                    .height(Size::px(26.0))
                                    .corner_radius(CornerRadius::new_all(7.0))
                                    .background(if is_cur {
                                        theme.rgba(ThemeRole::Accent).with_alpha(0.22).to_color()
                                    } else {
                                        subtle_bg
                                    })
                                    .border(Border::new().width(1.0).alignment(BorderAlignment::Inner).fill(
                                        if is_cur {
                                            theme.color(ThemeRole::Accent)
                                        } else {
                                            border_col
                                        },
                                    ))
                                    .center()
                                    .cursor(CursorIcon::Pointer)
                                    .on_press(move |e: Event<PressEventData>| {
                                        e.stop_propagation();
                                        core_ctx.invoke_or_log(Command::AgentSetContext {
                                            agent_id: Some(aid.clone()),
                                            context_id: cid.clone(),
                                        });
                                        context_menu_open.set(false);
                                    })
                                    .child(
                                        label()
                                            .text(choice.label.clone())
                                            .font_size(11.5)
                                            .font_weight(if is_cur {
                                                FontWeight::SEMI_BOLD
                                            } else {
                                                FontWeight::NORMAL
                                            })
                                            .color(theme.color(ThemeRole::Text)),
                                    ),
                            );
                    }

                    rect()
                        .width(Size::fill())
                        .spacing(4.0)
                        .child(
                            rect()
                                .a11y_role(AccessibilityRole::Button)
                                .a11y_alt("Choose context window")
                                .width(Size::fill())
                                .height(Size::px(26.0))
                                .padding(Gaps::new(0.0, 2.0, 0.0, 2.0))
                                .corner_radius(CornerRadius::new_all(6.0))
                                .direction(Direction::Horizontal)
                                .cross_align(Alignment::center())
                                .main_align(Alignment::SpaceBetween)
                                .cursor(CursorIcon::Pointer)
                                .on_press(move |e: Event<PressEventData>| {
                                    e.stop_propagation();
                                    let cur = *context_menu_open.peek();
                                    context_menu_open.set(!cur);
                                })
                                .child(
                                    label()
                                        .text("Context window")
                                        .font_size(12.0)
                                        .color(theme.color(ThemeRole::Text)),
                                )
                                .child(
                                    rect()
                                        .direction(Direction::Horizontal)
                                        .cross_align(Alignment::center())
                                        .spacing(5.0)
                                        .child(
                                            label()
                                                .text(active_label)
                                                .font_size(12.0)
                                                .color(theme.color(ThemeRole::TextMuted)),
                                        )
                                        .child(chevron_right_icon(10.0, theme.color(ThemeRole::TextMuted))),
                                ),
                        )
                        .maybe_child(expanded.then_some(choices_row))
                });

            rect()
                .width(Size::fill())
                .padding(Gaps::new(12.0, 14.0, 10.0, 14.0))
                .spacing(10.0)
                .child(
                    rect()
                        .width(Size::fill())
                        .spacing(2.0)
                        .child(
                            label()
                                .text(self.effective_effort.label())
                                .font_size(14.0)
                                .font_weight(FontWeight::BOLD)
                                .color(theme.color(ThemeRole::Text)),
                        )
                        .child(
                            rect()
                                .a11y_role(AccessibilityRole::Button)
                                .a11y_alt("Switch model")
                                .direction(Direction::Horizontal)
                                .cross_align(Alignment::center())
                                .spacing(4.0)
                                .cursor(CursorIcon::Pointer)
                                .on_press(move |e: Event<PressEventData>| {
                                    e.stop_propagation();
                                    if let Some(mut ds) = dock_state {
                                        ds.model_card_list_view.set(true);
                                    }
                                })
                                .child(
                                    label()
                                        .text(short_model_name(&self.selected_model.name).to_string())
                                        .font_size(11.5)
                                        .color(theme.color(ThemeRole::TextMuted))
                                        .max_lines(1),
                                )
                                .child(chevron_right_icon(10.0, theme.color(ThemeRole::TextMuted))),
                        ),
                )
                .child(effort_slider)
                .maybe_child(context_section)
                .into_element()
        };

        let mut card = rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_absolute().left(16.0).bottom(82.0))
            .width(Size::px(MODEL_CARD_WIDTH))
            .corner_radius(CornerRadius::new_all(14.0))
            .overflow(Overflow::Clip)
            .background(bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_col),
            )
            .shadow(theme.shadows.float_drop.to_shadow())
            .on_pointer_down(move |e: Event<PointerEventData>| {
                e.stop_propagation();
            })
            .child(content);

        if theme.glass.model_blur > 0.0 {
            card = card.backdrop_blur(theme.glass.model_blur);
        }
        card
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
