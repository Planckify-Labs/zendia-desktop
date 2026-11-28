/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Floating Model & Effort card popover anchored above (or below) the agent composer's model chip.
//!
//! Matches Zendia's `ZendiaAgentModelCard` layer by layer:
//! - Card shell: `256 px` wide, `18 px` radius, thin model glass (`11 px` blur), top specular sheen, `0.5 px` inner
//!   border, no drop shadow, `140 ms` open / `100 ms` close fade+slide, `200 ms` page crossfade.
//! - Main view: clickable full-width header (`48 px`, or `36 px` when no effort levels) with rolling effort heading
//!   (`"Default"` when unset) and model name + leaning chevron; unclipped `28 px` effort slider (`20 px` rail behind,
//!   unfilled stop dots, colored fill with top highlight + inset top line + clipped opposite-contrast stop dots +
//!   `0.5` unset opacity, `40×26 px` elevated white gradient thumb painting above everything with drag lift and
//!   focus ring); plus `Context window` row with floating `232 px` side submenu (or plain text when fixed).
//! - Searchable model list view: `32×32 px` back button, auto-focused `Search models…` `TextField`, prepended
//!   `"Default"` row, grouped section headings hidden while searching, capability tags (`text only` / `reads it` /
//!   `cannot read …`), controlled scroll list up to `246 px`, and `26 px` bottom detail bar.

use crate::ui::agent::motion::RollingText;
use crate::ui::agent::{agent_input_colors, chevron_left_icon, chevron_right_icon};
use crate::ui::core_view::use_core;
use crate::ui::kit::{
    Icon, KitTooltip, TextField, focus_ring_border, mix_color, scroll_item_into_view, use_interactive_feedback,
};
use crate::ui::panels::try_use_dock_state;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::agent::{
    AttachmentKind, ContextChoice, EFFORT_THUMB_INSET, effort_rail_fraction, effort_stop_index, format_tokens,
    model_media_tag, search_models,
};
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{AgentId, ModelEffort, ModelId, ModelInfo};

/// Width of the floating model card in logical pixels (`zendia.css:2856`).
pub const MODEL_CARD_WIDTH: f32 = 256.0;
const CARD_RADIUS: f32 = 18.0;
const CARD_INNER_RADIUS: f32 = 14.0;
const CHIP_GAP: f32 = 6.0;
const EDGE_MARGIN: f32 = 8.0;
const MIN_ROOM_ABOVE: f32 = 180.0;
const SUBMENU_WIDTH: f32 = 232.0;
const SUBMENU_GAP: f32 = 5.0;

const SLIDER_HEIGHT: f32 = 28.0;
const RAIL_HEIGHT: f32 = 20.0;
const RAIL_TOP: f32 = 4.0;
const THUMB_WIDTH: f32 = 40.0;
const THUMB_HEIGHT: f32 = 26.0;
const THUMB_TOP: f32 = 1.0;
const STOP_DOT_SIZE: f32 = 4.0;
const DEFAULT_RAIL_WIDTH: f32 = MODEL_CARD_WIDTH - 8.0 - 16.0; // 232 px
const LIST_MAX_HEIGHT: f32 = 246.0;
const LIST_ROW_HEIGHT: f32 = 32.0;

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

/// One displayed entry in the searchable model list (`model_id: None` is the prepended `"Default"` choice).
#[derive(Clone, PartialEq)]
struct ModelListEntry {
    model_id: Option<ModelId>,
    name: String,
    section: Option<String>,
    detail: String,
    tag: Option<(String, bool)>,
}

fn build_model_list_entries(
    agent_name: &str,
    models: &[ModelInfo],
    staged_kinds: &[AttachmentKind],
    query: &str,
) -> Vec<ModelListEntry> {
    let needle = query.trim().to_ascii_lowercase();
    let default_detail = format!("what {agent_name}'s settings pick");
    let first_section = models.first().and_then(|m| m.section.clone());
    let default_entry = ModelListEntry {
        model_id: None,
        name: "Default".to_string(),
        section: first_section,
        detail: default_detail.clone(),
        tag: None,
    };

    let mut out = Vec::new();
    let include_default =
        needle.is_empty() || "default".contains(&needle) || default_detail.to_ascii_lowercase().contains(&needle);
    if include_default {
        out.push(default_entry);
    }

    for model in search_models(models, query) {
        let detail = match &model.detail {
            Some(d) => format!("{} · {d}", model.id.0),
            None => format!("{} · {} context", model.id.0, format_tokens(model.context_window)),
        };
        out.push(ModelListEntry {
            model_id: Some(model.id.clone()),
            name: model.name.clone(),
            section: model.section.clone(),
            detail,
            tag: model_media_tag(model, staged_kinds),
        });
    }
    out
}

/// Anchored floating popover for choosing model, reasoning effort, and context window.
#[derive(Clone, PartialEq)]
pub struct AgentModelCard {
    pub open: bool,
    pub agent_id: AgentId,
    pub agent_name: String,
    pub selected_model: ModelInfo,
    pub explicit_model_id: Option<ModelId>,
    pub explicit_effort: Option<ModelEffort>,
    pub effective_effort: ModelEffort,
    pub active_context_id: Option<String>,
    pub models: Vec<ModelInfo>,
    pub staged_kinds: Vec<AttachmentKind>,
    pub accent: Color,
    pub composer_a11y_id: AccessibilityId,
    pub static_snapshot: bool,
    key: DiffKey,
}

impl KeyExt for AgentModelCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl AgentModelCard {
    pub fn new(
        open: bool,
        agent_id: AgentId,
        agent_name: String,
        selected_model: ModelInfo,
        effective_effort: ModelEffort,
        models: Vec<ModelInfo>,
        composer_a11y_id: AccessibilityId,
    ) -> Self {
        Self {
            open,
            agent_id,
            agent_name,
            selected_model,
            explicit_model_id: None,
            explicit_effort: None,
            effective_effort,
            active_context_id: None,
            models,
            staged_kinds: Vec::new(),
            accent: Color::TRANSPARENT,
            composer_a11y_id,
            static_snapshot: false,
            key: DiffKey::None,
        }
    }

    pub fn explicit_model_id(mut self, id: Option<ModelId>) -> Self {
        self.explicit_model_id = id;
        self
    }

    pub fn explicit_effort(mut self, effort: Option<ModelEffort>) -> Self {
        self.explicit_effort = effort;
        self
    }

    pub fn active_context_id(mut self, context_id: Option<String>) -> Self {
        self.active_context_id = context_id;
        self
    }

    pub fn staged_kinds(mut self, kinds: Vec<AttachmentKind>) -> Self {
        self.staged_kinds = kinds;
        self
    }

    pub fn accent(mut self, accent: Color) -> Self {
        self.accent = accent;
        self
    }

    pub fn static_snapshot(mut self, static_snapshot: bool) -> Self {
        self.static_snapshot = static_snapshot;
        self
    }
}

impl Component for AgentModelCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let dock_state = try_use_dock_state();

        let mut search_query = use_state(String::new);
        let mut submenu_open = use_state(|| false);
        let mut submenu_timer_seq = use_state(|| 0u64);
        let mut sub_active = use_state(|| 0usize);
        let mut list_active = use_state(|| 0usize);
        let mut last_list_view = use_state(|| false);
        let mut last_open = use_state(|| self.open);
        let mut effort_kbd_focus = use_state(|| false);
        let mut card_area = use_state(|| (0.0f32, 0.0f32, MODEL_CARD_WIDTH, 136.0f32));
        let mut context_row_y = use_state(|| 0.0f32);
        let mut list_scroll = use_scroll_controller(ScrollConfig::default);
        let slider_a11y_id = use_a11y();
        let search_a11y_id = use_a11y();

        let is_list_view = dock_state.is_some_and(|ds| *ds.model_card_list_view.read());
        let static_snap = self.static_snapshot;

        // Open/close animation: 140ms ease in, 100ms ease out (`zendia.css:2860,2875`).
        let open_anim = use_animation_with_dependencies(&self.open, move |conf, is_open| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            if *is_open {
                AnimNum::new(0.0, 1.0)
                    .duration(if static_snap {
                        Duration::ZERO
                    } else {
                        motion::MODEL_CARD_IN
                    })
                    .ease(Ease::Out)
                    .function(motion::css_ease_default_fn())
            } else {
                AnimNum::new(1.0, 0.0)
                    .duration(if static_snap {
                        Duration::ZERO
                    } else {
                        motion::MODEL_CARD_OUT
                    })
                    .ease(Ease::In)
                    .function(motion::css_ease_default_fn())
            }
        });

        // Page crossfade animation: 200ms cubic-bezier(0, 0, 0.58, 1) (`ZendiaAgentModelCard.sys.mjs:47`).
        let page_fade = use_animation_with_dependencies(&is_list_view, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(if static_snap {
                    Duration::ZERO
                } else {
                    motion::MODEL_PAGE_FADE
                })
                .ease(Ease::Out)
                .function(motion::css_ease_out_fn())
        });

        // Submenu entrance animation: 140ms (`zendia.css:2860,2879`).
        let is_sub_open = *submenu_open.read() && self.open && !is_list_view;
        let sub_anim = use_animation_with_dependencies(&is_sub_open, move |conf, shown| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            if *shown {
                AnimNum::new(0.0, 1.0)
                    .duration(if static_snap {
                        Duration::ZERO
                    } else {
                        motion::MODEL_CARD_IN
                    })
                    .ease(Ease::Out)
                    .function(motion::css_ease_default_fn())
            } else {
                AnimNum::new(0.0, 0.0).duration(Duration::ZERO)
            }
        });

        // Reset transient state whenever card opens/closes or flips page.
        if *last_open.peek() != self.open {
            last_open.set(self.open);
            submenu_open.set_if_modified(false);
            effort_kbd_focus.set_if_modified(false);
            if self.open {
                search_query.set_if_modified(String::new());
                if !is_list_view && !static_snap {
                    slider_a11y_id.request_focus();
                }
            }
        }
        if *last_list_view.peek() != is_list_view {
            last_list_view.set(is_list_view);
            submenu_open.set_if_modified(false);
            if is_list_view {
                search_query.set_if_modified(String::new());
                let entries = build_model_list_entries(&self.agent_name, &self.models, &self.staged_kinds, "");
                let initial = entries
                    .iter()
                    .position(|e| e.model_id == self.explicit_model_id)
                    .unwrap_or(0);
                list_active.set(initial);
                if self.open && !static_snap {
                    search_a11y_id.request_focus();
                }
            } else if self.open && !static_snap {
                slider_a11y_id.request_focus();
            }
        }

        let open_progress = open_anim.get().value().clamp(0.0, 1.0);
        if !self.open && open_progress <= 0.005 {
            return rect().width(Size::px(0.0)).height(Size::px(0.0));
        }

        let root_size = *Platform::get().current_window().root_size.read();
        let win_w = root_size.width.max(320.0);
        let win_h = root_size.height.max(240.0);
        let anchor = dock_state.and_then(|ds| *ds.model_chip_anchor.read());

        let efforts: Vec<ModelEffort> = if self.selected_model.efforts.len() > 1 {
            self.selected_model.efforts.clone()
        } else {
            Vec::new()
        };
        let has_slider = !efforts.is_empty();
        let has_context_choices = self.selected_model.context_choices.len() > 1;
        let has_context_row = has_context_choices || self.selected_model.context_window > 0;

        let query = search_query.read().clone();
        let list_entries = build_model_list_entries(&self.agent_name, &self.models, &self.staged_kinds, &query);
        let entry_count = list_entries.len();
        let active_list_idx = if entry_count == 0 {
            0
        } else {
            (*list_active.read()).min(entry_count - 1)
        };

        // Compute anchor placement (`ZendiaAgentModelCard.sys.mjs:248-266`).
        let (place_above, available_room, card_left, card_top) = if let Some((ax, ay, aw, ah)) = anchor {
            let above = (ay - CHIP_GAP - EDGE_MARGIN).max(0.0);
            let below = (win_h - (ay + ah) - CHIP_GAP - EDGE_MARGIN).max(0.0);
            let up = above >= MIN_ROOM_ABOVE || above >= below;
            let room = if up { above } else { below };
            let est_h = card_area.read().3.max(88.0).min(room.max(88.0));
            let left = (ax + aw - MODEL_CARD_WIDTH)
                .clamp(EDGE_MARGIN, (win_w - MODEL_CARD_WIDTH - EDGE_MARGIN).max(EDGE_MARGIN));
            let top = if up {
                (ay - CHIP_GAP - est_h).max(EDGE_MARGIN)
            } else {
                (ay + ah + CHIP_GAP).min((win_h - est_h - EDGE_MARGIN).max(EDGE_MARGIN))
            };
            (up, room, left, top)
        } else {
            // Fallback on first frame before `on_sized` reports chip bounds.
            let est_h = if is_list_view { 280.0 } else { 126.0 };
            let left = (win_w - MODEL_CARD_WIDTH - 24.0).max(EDGE_MARGIN);
            let top = (win_h - est_h - 92.0).max(EDGE_MARGIN);
            (true, (win_h - 120.0).max(180.0), left, top)
        };

        let travel = if place_above { 4.0f32 } else { -4.0f32 };
        let card_opacity = if self.open {
            0.3 + 0.7 * open_progress
        } else {
            open_progress
        };
        let card_offset_y = if self.open {
            travel * (1.0 - open_progress)
        } else {
            (travel * 0.5) * (1.0 - open_progress)
        };

        let card_bg = if theme.glass.model_blur > 0.0 {
            theme.glass.model_tint.to_color()
        } else {
            theme.glass.float_solid.to_color()
        };
        let border_col = theme.rgba(ThemeRole::Border).with_alpha(0.80).to_color();
        let divider_col = theme.color(ThemeRole::Border);
        let sheen_gradient = LinearGradient::new()
            .angle(0.0)
            .stop((theme.glass.sheen_top.to_color(), 0.0))
            .stop((Color::from_argb(0, 255, 255, 255), 100.0));

        // Keyboard handler (`MC-02` — standard keys only; vim keys ported separately).
        let is_open_flag = self.open;
        let efforts_for_keys = efforts.clone();
        let eff_effort_for_keys = self.effective_effort;
        let agent_for_keys = self.agent_id.clone();
        let contexts_for_keys = self.selected_model.context_choices.clone();
        let entries_for_keys = list_entries.clone();
        let composer_id = self.composer_a11y_id;
        let core_keys = core.clone();
        let list_max_h = LIST_MAX_HEIGHT.min((available_room - 78.0).max(96.0));

        let on_global_key = move |e: Event<KeyboardEventData>| {
            if !is_open_flag {
                return;
            }
            let plain = !e
                .modifiers
                .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META);
            let ctrl_only = e.modifiers == Modifiers::CONTROL;
            let is_ctrl_cancel = ctrl_only
                && (matches!(e.code, Code::KeyC | Code::BracketLeft)
                    || matches!(&e.key, Key::Character(c) if c.eq_ignore_ascii_case("c") || c == "["));
            let is_ctrl_next = ctrl_only
                && (e.code == Code::KeyN || matches!(&e.key, Key::Character(c) if c.eq_ignore_ascii_case("n")));
            let is_ctrl_prev = ctrl_only
                && (e.code == Code::KeyP || matches!(&e.key, Key::Character(c) if c.eq_ignore_ascii_case("p")));

            if *submenu_open.peek() && !is_list_view {
                let count = contexts_for_keys.len();
                if count == 0 {
                    e.stop_propagation();
                    e.prevent_default();
                    return;
                }
                if is_ctrl_cancel {
                    e.stop_propagation();
                    e.prevent_default();
                    submenu_open.set(false);
                    if let Some(mut ds) = dock_state {
                        ds.model_card_open.set_if_modified(true);
                    }
                    return;
                }
                if is_ctrl_next {
                    e.stop_propagation();
                    e.prevent_default();
                    let next = (*sub_active.peek() + 1) % count;
                    sub_active.set(next);
                    return;
                }
                if is_ctrl_prev {
                    e.stop_propagation();
                    e.prevent_default();
                    let cur = *sub_active.peek();
                    sub_active.set((cur + count - 1) % count);
                    return;
                }
                match &e.key {
                    Key::Named(NamedKey::Escape | NamedKey::ArrowLeft) => {
                        e.stop_propagation();
                        e.prevent_default();
                        submenu_open.set(false);
                        // Keep card open if DockFrame's global Escape handler cleared it in the same dispatch.
                        if let Some(mut ds) = dock_state {
                            ds.model_card_open.set_if_modified(true);
                        }
                    }
                    Key::Character(ch) if plain && ch == "h" => {
                        e.stop_propagation();
                        e.prevent_default();
                        submenu_open.set(false);
                        if let Some(mut ds) = dock_state {
                            ds.model_card_open.set_if_modified(true);
                        }
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        e.stop_propagation();
                        e.prevent_default();
                        let next = (*sub_active.peek() + 1) % count;
                        sub_active.set(next);
                    }
                    Key::Character(ch) if plain && ch == "j" => {
                        e.stop_propagation();
                        e.prevent_default();
                        let next = (*sub_active.peek() + 1) % count;
                        sub_active.set(next);
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        e.stop_propagation();
                        e.prevent_default();
                        let cur = *sub_active.peek();
                        sub_active.set((cur + count - 1) % count);
                    }
                    Key::Character(ch) if plain && ch == "k" => {
                        e.stop_propagation();
                        e.prevent_default();
                        let cur = *sub_active.peek();
                        sub_active.set((cur + count - 1) % count);
                    }
                    Key::Named(NamedKey::Home) => {
                        e.stop_propagation();
                        e.prevent_default();
                        sub_active.set(0);
                    }
                    Key::Named(NamedKey::End) => {
                        e.stop_propagation();
                        e.prevent_default();
                        sub_active.set(count - 1);
                    }
                    Key::Named(NamedKey::Enter) => {
                        e.stop_propagation();
                        e.prevent_default();
                        if let Some(choice) = contexts_for_keys.get(*sub_active.peek()) {
                            core_keys.invoke_or_log(Command::AgentSetContext {
                                agent_id: Some(agent_for_keys.clone()),
                                context_id: choice.id.clone(),
                            });
                        }
                        submenu_open.set(false);
                    }
                    Key::Character(ch) if ch == " " && plain => {
                        e.stop_propagation();
                        e.prevent_default();
                        if let Some(choice) = contexts_for_keys.get(*sub_active.peek()) {
                            core_keys.invoke_or_log(Command::AgentSetContext {
                                agent_id: Some(agent_for_keys.clone()),
                                context_id: choice.id.clone(),
                            });
                        }
                        submenu_open.set(false);
                    }
                    _ => {
                        e.stop_propagation();
                        e.prevent_default();
                    }
                }
                return;
            }

            if is_list_view {
                let count = entries_for_keys.len();
                if is_ctrl_cancel {
                    e.stop_propagation();
                    e.prevent_default();
                    if let Some(mut ds) = dock_state {
                        ds.model_card_list_view.set(false);
                        ds.model_card_open.set_if_modified(true);
                    }
                    return;
                }
                if is_ctrl_next && count > 0 {
                    e.stop_propagation();
                    e.prevent_default();
                    let next = (*list_active.peek() + 1) % count;
                    list_active.set(next);
                    let content_h = count as f32 * (LIST_ROW_HEIGHT + 2.0) + 32.0;
                    scroll_item_into_view(
                        &mut list_scroll,
                        next as f32 * (LIST_ROW_HEIGHT + 2.0),
                        LIST_ROW_HEIGHT,
                        list_max_h,
                        content_h,
                    );
                    return;
                }
                if is_ctrl_prev && count > 0 {
                    e.stop_propagation();
                    e.prevent_default();
                    let cur = *list_active.peek();
                    let next = (cur + count - 1) % count;
                    list_active.set(next);
                    let content_h = count as f32 * (LIST_ROW_HEIGHT + 2.0) + 32.0;
                    scroll_item_into_view(
                        &mut list_scroll,
                        next as f32 * (LIST_ROW_HEIGHT + 2.0),
                        LIST_ROW_HEIGHT,
                        list_max_h,
                        content_h,
                    );
                    return;
                }
                match &e.key {
                    Key::Named(NamedKey::Escape) => {
                        e.stop_propagation();
                        e.prevent_default();
                        if let Some(mut ds) = dock_state {
                            ds.model_card_list_view.set(false);
                            ds.model_card_open.set_if_modified(true);
                        }
                    }
                    Key::Named(NamedKey::ArrowDown) if count > 0 => {
                        e.stop_propagation();
                        e.prevent_default();
                        let next = (*list_active.peek() + 1) % count;
                        list_active.set(next);
                        let content_h = count as f32 * (LIST_ROW_HEIGHT + 2.0) + 32.0;
                        scroll_item_into_view(
                            &mut list_scroll,
                            next as f32 * (LIST_ROW_HEIGHT + 2.0),
                            LIST_ROW_HEIGHT,
                            list_max_h,
                            content_h,
                        );
                    }
                    Key::Named(NamedKey::ArrowUp) if count > 0 => {
                        e.stop_propagation();
                        e.prevent_default();
                        let cur = *list_active.peek();
                        let next = (cur + count - 1) % count;
                        list_active.set(next);
                        let content_h = count as f32 * (LIST_ROW_HEIGHT + 2.0) + 32.0;
                        scroll_item_into_view(
                            &mut list_scroll,
                            next as f32 * (LIST_ROW_HEIGHT + 2.0),
                            LIST_ROW_HEIGHT,
                            list_max_h,
                            content_h,
                        );
                    }
                    Key::Named(NamedKey::Home) if count > 0 && !search_a11y_id.is_focused() => {
                        e.stop_propagation();
                        e.prevent_default();
                        list_active.set(0);
                        let content_h = count as f32 * (LIST_ROW_HEIGHT + 2.0) + 32.0;
                        scroll_item_into_view(&mut list_scroll, 0.0, LIST_ROW_HEIGHT, list_max_h, content_h);
                    }
                    Key::Named(NamedKey::End) if count > 0 && !search_a11y_id.is_focused() => {
                        e.stop_propagation();
                        e.prevent_default();
                        let last = count - 1;
                        list_active.set(last);
                        let content_h = count as f32 * (LIST_ROW_HEIGHT + 2.0) + 32.0;
                        scroll_item_into_view(
                            &mut list_scroll,
                            last as f32 * (LIST_ROW_HEIGHT + 2.0),
                            LIST_ROW_HEIGHT,
                            list_max_h,
                            content_h,
                        );
                    }
                    Key::Named(NamedKey::Enter) if count > 0 => {
                        e.stop_propagation();
                        e.prevent_default();
                        let idx = (*list_active.peek()).min(count - 1);
                        if let Some(entry) = entries_for_keys.get(idx) {
                            core_keys.invoke_or_log(Command::AgentModel {
                                agent_id: Some(agent_for_keys.clone()),
                                model_id: entry.model_id.clone(),
                            });
                            if let Some(mut ds) = dock_state {
                                ds.model_card_list_view.set(false);
                            }
                        }
                    }
                    _ => {
                        e.stop_propagation();
                    }
                }
                return;
            }

            // Main view keys (`AGK-02`).
            if is_ctrl_cancel {
                e.stop_propagation();
                e.prevent_default();
                if let Some(mut ds) = dock_state {
                    ds.model_card_open.set(false);
                    ds.model_card_list_view.set(false);
                }
                composer_id.request_focus();
                return;
            }
            let stepping = efforts_for_keys.len() > 1;
            let cur_idx = efforts_for_keys
                .iter()
                .position(|ef| *ef == eff_effort_for_keys)
                .unwrap_or(0);
            match &e.key {
                Key::Named(NamedKey::Escape) => {
                    e.stop_propagation();
                    e.prevent_default();
                    if let Some(mut ds) = dock_state {
                        ds.model_card_open.set(false);
                        ds.model_card_list_view.set(false);
                    }
                    composer_id.request_focus();
                }
                Key::Named(NamedKey::ArrowLeft) if stepping => {
                    e.stop_propagation();
                    e.prevent_default();
                    effort_kbd_focus.set_if_modified(true);
                    let next = cur_idx.saturating_sub(1);
                    if let Some(&effort) = efforts_for_keys.get(next) {
                        core_keys.invoke_or_log(Command::AgentSetEffort {
                            agent_id: Some(agent_for_keys.clone()),
                            effort: Some(effort),
                        });
                    }
                }
                Key::Character(ch) if plain && ch == "h" && stepping => {
                    e.stop_propagation();
                    e.prevent_default();
                    effort_kbd_focus.set_if_modified(true);
                    let next = cur_idx.saturating_sub(1);
                    if let Some(&effort) = efforts_for_keys.get(next) {
                        core_keys.invoke_or_log(Command::AgentSetEffort {
                            agent_id: Some(agent_for_keys.clone()),
                            effort: Some(effort),
                        });
                    }
                }
                Key::Named(NamedKey::ArrowRight) if stepping => {
                    e.stop_propagation();
                    e.prevent_default();
                    effort_kbd_focus.set_if_modified(true);
                    let next = (cur_idx + 1).min(efforts_for_keys.len() - 1);
                    if let Some(&effort) = efforts_for_keys.get(next) {
                        core_keys.invoke_or_log(Command::AgentSetEffort {
                            agent_id: Some(agent_for_keys.clone()),
                            effort: Some(effort),
                        });
                    }
                }
                Key::Character(ch) if plain && ch == "l" && stepping => {
                    e.stop_propagation();
                    e.prevent_default();
                    effort_kbd_focus.set_if_modified(true);
                    let next = (cur_idx + 1).min(efforts_for_keys.len() - 1);
                    if let Some(&effort) = efforts_for_keys.get(next) {
                        core_keys.invoke_or_log(Command::AgentSetEffort {
                            agent_id: Some(agent_for_keys.clone()),
                            effort: Some(effort),
                        });
                    }
                }
                Key::Named(NamedKey::Home) if stepping => {
                    e.stop_propagation();
                    e.prevent_default();
                    effort_kbd_focus.set_if_modified(true);
                    if let Some(&effort) = efforts_for_keys.first() {
                        core_keys.invoke_or_log(Command::AgentSetEffort {
                            agent_id: Some(agent_for_keys.clone()),
                            effort: Some(effort),
                        });
                    }
                }
                Key::Character(ch) if plain && ch == "0" && stepping => {
                    e.stop_propagation();
                    e.prevent_default();
                    effort_kbd_focus.set_if_modified(true);
                    if let Some(&effort) = efforts_for_keys.first() {
                        core_keys.invoke_or_log(Command::AgentSetEffort {
                            agent_id: Some(agent_for_keys.clone()),
                            effort: Some(effort),
                        });
                    }
                }
                Key::Named(NamedKey::End) if stepping => {
                    e.stop_propagation();
                    e.prevent_default();
                    effort_kbd_focus.set_if_modified(true);
                    if let Some(&effort) = efforts_for_keys.last() {
                        core_keys.invoke_or_log(Command::AgentSetEffort {
                            agent_id: Some(agent_for_keys.clone()),
                            effort: Some(effort),
                        });
                    }
                }
                Key::Character(ch) if plain && ch == "$" && stepping => {
                    e.stop_propagation();
                    e.prevent_default();
                    effort_kbd_focus.set_if_modified(true);
                    if let Some(&effort) = efforts_for_keys.last() {
                        core_keys.invoke_or_log(Command::AgentSetEffort {
                            agent_id: Some(agent_for_keys.clone()),
                            effort: Some(effort),
                        });
                    }
                }
                Key::Character(ch) if plain && ch.eq_ignore_ascii_case("c") && contexts_for_keys.len() > 1 => {
                    e.stop_propagation();
                    e.prevent_default();
                    submenu_open.set(true);
                }
                Key::Named(NamedKey::Enter | NamedKey::ArrowDown | NamedKey::ArrowUp) => {
                    e.stop_propagation();
                    e.prevent_default();
                    if let Some(mut ds) = dock_state {
                        ds.model_card_list_view.set(true);
                    }
                }
                Key::Character(ch) if plain && matches!(ch.as_str(), " " | "j" | "k") => {
                    e.stop_propagation();
                    e.prevent_default();
                    if let Some(mut ds) = dock_state {
                        ds.model_card_list_view.set(true);
                    }
                }
                _ => {
                    e.stop_propagation();
                    e.prevent_default();
                }
            }
        };

        // --- Build active page ---
        let page_opacity = page_fade.get().value().clamp(0.0, 1.0);
        let page_content: Element = if is_list_view {
            let grouped = query.trim().is_empty();
            let detail_text = list_entries
                .get(active_list_idx)
                .map(|e| e.detail.clone())
                .unwrap_or_default();

            let mut list_col = rect().width(Size::fill()).spacing(2.0);
            let mut last_section: Option<String> = None;
            let mut section_count = 0usize;

            if list_entries.is_empty() {
                list_col = list_col.child(
                    rect().width(Size::fill()).padding(Gaps::new_all(8.0)).child(
                        label()
                            .text("No model matches")
                            .font_size(12.0)
                            .opacity(0.72)
                            .color(theme.color(ThemeRole::Text)),
                    ),
                );
            } else {
                for (idx, entry) in list_entries.iter().enumerate() {
                    if grouped
                        && let Some(sec) = &entry.section
                        && last_section.as_deref() != Some(sec.as_str())
                    {
                        let top_pad = if section_count == 0 { 2.0 } else { 8.0 };
                        section_count += 1;
                        last_section = Some(sec.clone());
                        list_col = list_col.child(
                            rect()
                                .width(Size::fill())
                                .padding(Gaps::new(top_pad, 8.0, 2.0, 8.0))
                                .child(
                                    label()
                                        .text(sec.clone())
                                        .font_size(11.0)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .opacity(0.6)
                                        .color(theme.color(ThemeRole::Text)),
                                ),
                        );
                    }

                    let is_selected = entry.model_id == self.explicit_model_id;
                    let is_active = idx == active_list_idx;
                    let mid_pick = entry.model_id.clone();
                    let agent_pick = self.agent_id.clone();
                    let core_pick = core.clone();

                    list_col = list_col.child(
                        ModelListRow {
                            name: entry.name.clone(),
                            tag: entry.tag.clone(),
                            is_selected,
                            is_active,
                            on_hover: EventHandler::new(move |_| {
                                if *list_active.peek() != idx {
                                    list_active.set(idx);
                                }
                            }),
                            on_pick: EventHandler::new(move |_| {
                                core_pick.invoke_or_log(Command::AgentModel {
                                    agent_id: Some(agent_pick.clone()),
                                    model_id: mid_pick.clone(),
                                });
                                if let Some(mut ds) = dock_state {
                                    ds.model_card_list_view.set(false);
                                }
                            }),
                            key: DiffKey::None,
                        }
                        .key(format!("mrow:{idx}:{}", entry.name)),
                    );
                }
            }

            // Dynamic height: fit content cleanly up to `list_max_h` (`246px` / 7 rows).
            let estimated_content_h = if list_entries.is_empty() {
                34.0
            } else {
                list_entries.len() as f32 * (LIST_ROW_HEIGHT + 2.0) + section_count as f32 * 22.0 + 4.0
            };
            let list_h = estimated_content_h.min(list_max_h);

            let input_colors = agent_input_colors(&theme, true);
            let pref_color = |pref: Option<Preference<Color>>, fallback: Color| match pref {
                Some(Preference::Specific(c)) => c,
                _ => fallback,
            };

            let search_field = TextField::new(search_query)
                .a11y_id(search_a11y_id)
                .auto_focus(true)
                .placeholder("Search models…")
                .width(Size::fill())
                .height(Size::px(32.0))
                .color(pref_color(input_colors.color, theme.color(ThemeRole::Text)))
                .placeholder_color(pref_color(
                    input_colors.placeholder_color,
                    theme.color(ThemeRole::TextMuted),
                ))
                .padding(Gaps::new(0.0, 4.0, 0.0, 4.0))
                .on_change(move |_| {
                    list_active.set(0);
                });

            rect()
                .width(Size::fill())
                .padding(Gaps::new_all(4.0))
                .opacity(page_opacity)
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(36.0))
                        .padding(Gaps::new(0.0, 0.0, 4.0, 0.0))
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .cross_align(Alignment::center())
                        .spacing(4.0)
                        .child(ModelBackButton {
                            on_back: EventHandler::new(move |_| {
                                if let Some(mut ds) = dock_state {
                                    ds.model_card_list_view.set(false);
                                }
                            }),
                        })
                        .child(rect().width(Size::flex(1.0)).font_size(13.0).child(search_field)),
                )
                .child(rect().width(Size::fill()).height(Size::px(0.5)).background(divider_col))
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(list_h))
                        .margin(Gaps::new(4.0, 0.0, 0.0, 0.0))
                        .child(
                            ScrollView::new_controlled(list_scroll)
                                .width(Size::fill())
                                .height(Size::fill())
                                .child(list_col),
                        ),
                )
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(0.5))
                        .margin(Gaps::new(4.0, 0.0, 0.0, 0.0))
                        .background(divider_col),
                )
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(26.0))
                        .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .child(
                            label()
                                .text(detail_text)
                                .font_size(11.0)
                                .color(theme.color(ThemeRole::TextMuted))
                                .max_lines(1)
                                .text_overflow(TextOverflow::Ellipsis),
                        ),
                )
                .into_element()
        } else {
            let effort_heading = self
                .explicit_effort
                .map(|e| e.label().to_string())
                .unwrap_or_else(|| "Default".to_string());

            let active_ctx = self
                .active_context_id
                .as_deref()
                .and_then(|id| self.selected_model.context_choices.iter().find(|c| c.id == id))
                .or_else(|| self.selected_model.context_choices.iter().find(|c| c.is_default))
                .or_else(|| self.selected_model.context_choices.first());
            let context_label = active_ctx
                .map(|c| c.label.clone())
                .unwrap_or_else(|| format_tokens(self.selected_model.context_window));

            let mut open_sub = move || {
                let next_seq = submenu_timer_seq.peek().wrapping_add(1);
                submenu_timer_seq.set(next_seq);
                if !*submenu_open.peek() {
                    submenu_open.set(true);
                }
            };
            let mut linger_sub = move || {
                let seq = submenu_timer_seq.peek().wrapping_add(1);
                submenu_timer_seq.set(seq);
                spawn(async move {
                    timer(motion::MODEL_SUBMENU_LINGER).await;
                    if *submenu_timer_seq.peek() == seq {
                        submenu_open.set_if_modified(false);
                    }
                });
            };
            let mut toggle_sub = move || {
                let next_seq = submenu_timer_seq.peek().wrapping_add(1);
                submenu_timer_seq.set(next_seq);
                let cur = *submenu_open.peek();
                submenu_open.set(!cur);
            };

            rect()
                .width(Size::fill())
                .padding(Gaps::new_all(4.0))
                .opacity(page_opacity)
                .child(ModelTitleRow {
                    effort_heading: has_slider.then_some(effort_heading),
                    model_name: self.selected_model.name.clone(),
                    on_open_list: EventHandler::new(move |_| {
                        if let Some(mut ds) = dock_state {
                            ds.model_card_list_view.set(true);
                        }
                    }),
                })
                .maybe_child(has_slider.then(|| {
                    EffortSlider {
                        agent_id: self.agent_id.clone(),
                        efforts: efforts.clone(),
                        effective_effort: self.effective_effort,
                        is_unset: self.explicit_effort.is_none(),
                        keyboard_focused: *effort_kbd_focus.read(),
                        on_pointer_interact: EventHandler::new(move |_| {
                            effort_kbd_focus.set_if_modified(false);
                        }),
                        slider_a11y_id,
                        static_snapshot: static_snap,
                        key: DiffKey::None,
                    }
                    .key(format!("effort-slider:{}", self.selected_model.id.0))
                }))
                .maybe_child(has_context_row.then(|| ContextOptionRow {
                    value_label: context_label,
                    has_choices: has_context_choices,
                    submenu_open: is_sub_open,
                    on_enter: EventHandler::new(move |_| open_sub()),
                    on_leave: EventHandler::new(move |_| linger_sub()),
                    on_toggle: EventHandler::new(move |_| toggle_sub()),
                    on_row_sized: EventHandler::new(move |y: f32| {
                        context_row_y.set_if_modified(y);
                    }),
                }))
                .into_element()
        };

        // --- Floating Context Window Side Submenu (`zendia.css:2878-2885, 3125-3153`) ---
        let submenu_el = is_sub_open.then(|| {
            let sub_p = sub_anim.get().value().clamp(0.0, 1.0);
            let sub_opacity = 0.3 + 0.7 * sub_p;
            let sub_offset_y = -2.0 * (1.0 - sub_p);
            let (cx, _, cw, _) = *card_area.read();
            let card_left_actual = if cw > 10.0 { cx } else { card_left };
            let card_right_actual = card_left_actual + MODEL_CARD_WIDTH;
            let fits_right = card_right_actual + SUBMENU_GAP + SUBMENU_WIDTH + EDGE_MARGIN < win_w;
            let sub_left = if fits_right {
                card_right_actual + SUBMENU_GAP
            } else {
                (card_left_actual - SUBMENU_GAP - SUBMENU_WIDTH).max(EDGE_MARGIN)
            };
            let sub_est_h = self.selected_model.context_choices.len() as f32 * 32.0 + 8.0;
            let row_y = *context_row_y.read();
            let raw_top = if row_y > 1.0 { row_y } else { card_top + 86.0 };
            let sub_top = raw_top.min((win_h - sub_est_h - EDGE_MARGIN).max(EDGE_MARGIN));

            let active_ctx_id = self
                .active_context_id
                .as_deref()
                .or_else(|| {
                    self.selected_model
                        .context_choices
                        .iter()
                        .find(|c| c.is_default)
                        .map(|c| c.id.as_str())
                })
                .or_else(|| self.selected_model.context_choices.first().map(|c| c.id.as_str()));

            let mut choices_col = rect().width(Size::fill()).padding(Gaps::new_all(4.0)).spacing(2.0);
            for (idx, choice) in self.selected_model.context_choices.iter().enumerate() {
                let is_selected = active_ctx_id == Some(choice.id.as_str());
                let is_active = *sub_active.read() == idx;
                let aid = self.agent_id.clone();
                let cid = choice.id.clone();
                let core_ctx = core.clone();
                choices_col = choices_col.child(
                    ContextChoiceRow {
                        choice: choice.clone(),
                        is_selected,
                        is_active,
                        on_hover: EventHandler::new(move |_| sub_active.set_if_modified(idx)),
                        on_pick: EventHandler::new(move |_| {
                            core_ctx.invoke_or_log(Command::AgentSetContext {
                                agent_id: Some(aid.clone()),
                                context_id: cid.clone(),
                            });
                            submenu_open.set(false);
                        }),
                        key: DiffKey::None,
                    }
                    .key(format!("ctx:{}", choice.id)),
                );
            }

            let mut sub_card = rect()
                .layer(Layer::Relative(6))
                .position(Position::new_global().left(sub_left).top(sub_top))
                .width(Size::px(SUBMENU_WIDTH))
                .corner_radius(CornerRadius::new_all(CARD_RADIUS))
                .overflow(Overflow::Clip)
                .background(card_bg)
                .border(
                    Border::new()
                        .width(0.5)
                        .alignment(BorderAlignment::Inner)
                        .fill(border_col),
                )
                .offset_y(sub_offset_y)
                .opacity(sub_opacity)
                .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                .on_pointer_enter(move |_| {
                    let seq = submenu_timer_seq.peek().wrapping_add(1);
                    submenu_timer_seq.set(seq);
                })
                .on_pointer_leave(move |_| {
                    let seq = submenu_timer_seq.peek().wrapping_add(1);
                    submenu_timer_seq.set(seq);
                    spawn(async move {
                        timer(motion::MODEL_SUBMENU_LINGER).await;
                        if *submenu_timer_seq.peek() == seq {
                            submenu_open.set_if_modified(false);
                        }
                    });
                })
                .child(
                    rect()
                        .position(Position::new_absolute().top(0.0).left(0.0))
                        .width(Size::percent(100.0))
                        .height(Size::px(96.0))
                        .background(sheen_gradient.clone()),
                )
                .child(choices_col);

            if theme.glass.model_blur > 0.0 {
                sub_card = sub_card.backdrop_blur(theme.glass.model_blur);
            }
            sub_card
        });

        let mut card = rect()
            .layer(Layer::Relative(4))
            .position(Position::new_global().left(card_left).top(card_top))
            .width(Size::px(MODEL_CARD_WIDTH))
            .max_height(Size::px(available_room.max(96.0)))
            .corner_radius(CornerRadius::new_all(CARD_RADIUS))
            .overflow(Overflow::Clip)
            .background(card_bg)
            .border(
                Border::new()
                    .width(0.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_col),
            )
            .offset_y(card_offset_y)
            .opacity(card_opacity)
            .on_sized(move |e: Event<SizedEventData>| {
                let next = (e.area.min_x(), e.area.min_y(), e.area.width(), e.area.height());
                let prev = *card_area.peek();
                if (prev.0 - next.0).abs() > 0.5
                    || (prev.1 - next.1).abs() > 0.5
                    || (prev.2 - next.2).abs() > 0.5
                    || (prev.3 - next.3).abs() > 0.5
                {
                    card_area.set(next);
                }
            })
            .on_pointer_down(|e: Event<PointerEventData>| {
                e.stop_propagation();
            })
            .child(
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .width(Size::percent(100.0))
                    .height(Size::px(96.0))
                    .background(sheen_gradient),
            )
            .child(page_content);

        if theme.glass.model_blur > 0.0 {
            card = card.backdrop_blur(theme.glass.model_blur);
        }

        // Global overlay host: transparent click-away backdrop (layer 0), card (layer 4), submenu (layer 6).
        rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_global().top(0.0).left(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .on_global_key_down(on_global_key)
            .maybe_child(self.open.then(|| {
                rect()
                    .layer(Layer::Relative(0))
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .width(Size::percent(100.0))
                    .height(Size::percent(100.0))
                    .on_pointer_down(move |e: Event<PointerEventData>| {
                        e.stop_propagation();
                        submenu_open.set_if_modified(false);
                        if let Some(mut ds) = dock_state {
                            ds.model_card_open.set(false);
                            ds.model_card_list_view.set(false);
                        }
                    })
            }))
            .child(card)
            .maybe_child(submenu_el)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Interactive Effort slider matching `.zendia-effort`, `-stops`, `-stop`, `-fill`, and `-thumb` (`zendia.css:2979-3082`).
#[derive(Clone, PartialEq)]
struct EffortSlider {
    agent_id: AgentId,
    efforts: Vec<ModelEffort>,
    effective_effort: ModelEffort,
    is_unset: bool,
    keyboard_focused: bool,
    on_pointer_interact: EventHandler<()>,
    slider_a11y_id: AccessibilityId,
    static_snapshot: bool,
    key: DiffKey,
}

impl KeyExt for EffortSlider {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for EffortSlider {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let focus = use_focus(self.slider_a11y_id);

        let mut rail_area = use_state(|| (0.0f32, DEFAULT_RAIL_WIDTH));
        let mut dragging = use_state(|| false);
        let mut drag_frac = use_state(|| None::<f32>);
        let mut last_click_ms = use_state(|| 0u128);

        let stops = self.efforts.len().max(2);
        let active_idx = self
            .efforts
            .iter()
            .position(|e| *e == self.effective_effort)
            .unwrap_or(0);
        let target_frac = active_idx as f32 / (stops - 1) as f32;
        let target_color = theme.color(effort_role(self.effective_effort));

        let mut anim_from_frac = use_state(|| target_frac);
        let mut last_target_frac = use_state(|| target_frac);
        let mut anim_from_color = use_state(|| target_color);
        let mut last_target_color = use_state(|| target_color);
        let mut slide_seq = use_state(|| 0u64);

        let is_dragging = *dragging.read();
        let static_snap = self.static_snapshot;

        // Track fraction changes when not dragging so clicks, arrow keys, double-click unset, and drag-release all
        // glide smoothly from the prior position to the new stop over `200ms` (`EFFORT_SLIDE`).
        if !is_dragging && (*last_target_frac.peek() - target_frac).abs() > 1e-4 {
            let from = drag_frac.peek().unwrap_or(*last_target_frac.peek());
            anim_from_frac.set(from);
            last_target_frac.set(target_frac);
            drag_frac.set_if_modified(None);
            let next_seq = slide_seq.peek().wrapping_add(1);
            slide_seq.set(next_seq);
        }

        if *last_target_color.peek() != target_color {
            let prev = *last_target_color.peek();
            anim_from_color.set(prev);
            last_target_color.set(target_color);
        }

        let slide_anim =
            use_animation_with_dependencies(&(*slide_seq.read(), target_frac.to_bits()), move |conf, _| {
                conf.on_creation(OnCreation::Finish);
                conf.on_change(OnChange::Rerun);
                AnimNum::new(0.0, 1.0)
                    .duration(if static_snap {
                        Duration::ZERO
                    } else {
                        motion::EFFORT_SLIDE
                    })
                    .ease(Ease::Out)
                    .function(motion::css_ease_out_fn())
            });

        let color_anim = use_animation_with_dependencies(&self.effective_effort, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(if static_snap {
                    Duration::ZERO
                } else {
                    motion::EFFORT_SLIDE
                })
                .ease(Ease::Out)
                .function(motion::css_ease_out_fn())
        });

        let unset_anim = use_animation_with_dependencies(&self.is_unset, move |conf, is_unset| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *is_unset { (1.0, 0.5) } else { (0.5, 1.0) };
            AnimNum::new(from, to)
                .duration(if static_snap {
                    Duration::ZERO
                } else {
                    motion::MODEL_HOVER
                })
                .ease(Ease::InOut)
                .function(motion::css_ease_standard_fn())
        });

        let visual_frac = if is_dragging {
            drag_frac.read().unwrap_or(target_frac)
        } else {
            let p = slide_anim.get().value().clamp(0.0, 1.0);
            let from = *anim_from_frac.read();
            from + (target_frac - from) * p
        }
        .clamp(0.0, 1.0);

        let color_p = color_anim.get().value().clamp(0.0, 1.0);
        let fill_color = mix_color(*anim_from_color.read(), target_color, color_p);
        let fill_opacity = unset_anim.get().value().clamp(0.5, 1.0);

        let rail_w = rail_area.read().1.max(80.0);
        let span = (rail_w - THUMB_WIDTH).max(0.0);
        let thumb_left = visual_frac * span;
        // `zendia.css:3037`: `width: calc(32px + var(--zendia-effort-f) * var(--zendia-effort-span))`
        // Ends under the 40 px thumb so the thumb cleanly hides the rounded right end of the fill.
        let fill_w = (32.0 + visual_frac * span).clamp(RAIL_HEIGHT, rail_w);

        // Layer 1: Rail (`zendia.css:2995-3002`)
        let rail_bg = theme.rgba(ThemeRole::Border).with_alpha(0.38).to_color();
        let rail_border = theme.rgba(ThemeRole::Border).with_alpha(0.60).to_color();
        let rail_el = rect()
            .layer(Layer::Relative(0))
            .position(Position::new_absolute().top(RAIL_TOP).left(0.0))
            .width(Size::percent(100.0))
            .height(Size::px(RAIL_HEIGHT))
            .corner_radius(CornerRadius::new_all(999.0))
            .background(rail_bg)
            .border(
                Border::new()
                    .width(0.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(rail_border),
            );

        // Layer 2: Unfilled stop dots (`zendia.css:3005-3024`)
        let unfilled_dot_color = theme.rgba(ThemeRole::TextMuted).with_alpha(0.55).to_color();
        let mut unfilled_stops = rect()
            .layer(Layer::Relative(1))
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::percent(100.0))
            .height(Size::px(SLIDER_HEIGHT));
        for i in 0..stops {
            let stop_f = i as f32 / (stops - 1) as f32;
            let dot_left = EFFORT_THUMB_INSET + stop_f * span - STOP_DOT_SIZE * 0.5;
            unfilled_stops = unfilled_stops.child(
                rect()
                    .position(Position::new_absolute().top(12.0).left(dot_left))
                    .width(Size::px(STOP_DOT_SIZE))
                    .height(Size::px(STOP_DOT_SIZE))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(unfilled_dot_color),
            );
        }

        // Layer 3: Colored fill + top sheen + inset top line + clipped opposite-contrast dots (`zendia.css:3027-3052`)
        let filled_dot_color = match theme.scheme {
            ResolvedScheme::Light => Color::from_argb(153, 255, 255, 255), // rgba(255,255,255,0.6)
            ResolvedScheme::Dark => Color::from_argb(102, 27, 29, 43),     // rgba(27,29,43,0.4)
        };
        let fill_sheen = LinearGradient::new()
            .angle(0.0)
            .stop((Color::from_argb(41, 255, 255, 255), 0.0)) // rgba(255,255,255,0.16)
            .stop((Color::from_argb(0, 255, 255, 255), 100.0));

        let mut filled_stops = rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::px(rail_w))
            .height(Size::px(RAIL_HEIGHT));
        for i in 0..stops {
            let stop_f = i as f32 / (stops - 1) as f32;
            let dot_left = EFFORT_THUMB_INSET + stop_f * span - STOP_DOT_SIZE * 0.5;
            filled_stops = filled_stops.child(
                rect()
                    .position(Position::new_absolute().top(12.0 - RAIL_TOP).left(dot_left))
                    .width(Size::px(STOP_DOT_SIZE))
                    .height(Size::px(STOP_DOT_SIZE))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(filled_dot_color),
            );
        }

        let fill_el = rect()
            .layer(Layer::Relative(4))
            .position(Position::new_absolute().top(RAIL_TOP).left(0.0))
            .width(Size::px(fill_w))
            .height(Size::px(RAIL_HEIGHT))
            .corner_radius(CornerRadius::new_all(999.0))
            .overflow(Overflow::Clip)
            .background(fill_color)
            .shadow(
                Shadow::new()
                    .y(1.0)
                    .blur(0.0)
                    .color(Color::from_argb(56, 255, 255, 255))
                    .inset(),
            )
            .opacity(fill_opacity)
            .child(
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .width(Size::percent(100.0))
                    .height(Size::percent(100.0))
                    .background(fill_sheen),
            )
            .child(filled_stops);

        // Layer 4: Keyboard focus ring (`zendia.css:3078-3081`, 2px solid effort-color, 1px offset)
        let show_focus_ring = self.keyboard_focused || matches!(focus(), Focus::Keyboard);
        let focus_ring_el = show_focus_ring.then(|| {
            rect()
                .layer(Layer::Relative(8))
                .position(Position::new_absolute().top(THUMB_TOP - 3.0).left(thumb_left - 3.0))
                .width(Size::px(THUMB_WIDTH + 6.0))
                .height(Size::px(THUMB_HEIGHT + 6.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .border(
                    Border::new()
                        .width(2.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(fill_color),
                )
        });

        // Layer 5 (TOP): Elevated 40×26 white gradient thumb (`zendia.css:3056-3076`).
        let thumb_gradient = LinearGradient::new()
            .angle(0.0)
            .stop((Color::from_rgb(255, 255, 255), 0.0))
            .stop((Color::from_rgb(246, 246, 246), 100.0));
        let mut thumb_el = rect()
            .layer(Layer::Relative(10))
            .position(Position::new_absolute().top(THUMB_TOP).left(thumb_left))
            .width(Size::px(THUMB_WIDTH))
            .height(Size::px(THUMB_HEIGHT))
            .corner_radius(CornerRadius::new_all(999.0))
            .background(thumb_gradient)
            .shadow(
                Shadow::new()
                    .y(1.0)
                    .blur(0.0)
                    .color(Color::from_argb(204, 255, 255, 255))
                    .inset(),
            )
            .shadow(Shadow::new().spread(0.5).blur(0.0).color(Color::from_argb(31, 0, 0, 0)))
            .shadow(Shadow::new().y(1.0).blur(2.0).color(Color::from_argb(51, 0, 0, 0)));
        if is_dragging {
            thumb_el = thumb_el.shadow(Shadow::new().y(2.0).blur(8.0).color(Color::from_argb(36, 0, 0, 0)));
        }

        // Pointer drag handlers (`ZendiaAgentModelCard.sys.mjs:387-410, 429-436`).
        let efforts_down = self.efforts.clone();
        let efforts_move = self.efforts.clone();
        let agent_down = self.agent_id.clone();
        let agent_move = self.agent_id.clone();
        let core_down = core.clone();
        let core_move = core.clone();
        let slider_id_down = self.slider_a11y_id;
        let current_eff = self.effective_effort;
        let was_unset = self.is_unset;
        let on_ptr_down = self.on_pointer_interact.clone();

        let slider_box = rect().width(Size::fill()).padding(Gaps::new(2.0, 8.0, 4.0, 8.0)).child(
            rect()
                .a11y_id(self.slider_a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Slider)
                .a11y_alt(format!("Reasoning effort: {}", self.effective_effort.label()))
                .width(Size::fill())
                .height(Size::px(SLIDER_HEIGHT))
                .cursor(CursorIcon::Pointer)
                .on_sized(move |e: Event<SizedEventData>| {
                    rail_area.set_if_modified((e.area.min_x(), e.area.width()));
                })
                .on_pointer_down(move |e: Event<PointerEventData>| {
                    if e.button() != Some(MouseButton::Left) {
                        return;
                    }
                    e.stop_propagation();
                    on_ptr_down.call(());
                    slider_id_down.request_focus();
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    let prev = *last_click_ms.peek();
                    last_click_ms.set(now);
                    if now.saturating_sub(prev) < 350 {
                        dragging.set(false);
                        drag_frac.set(None);
                        core_down.invoke_or_log(Command::AgentSetEffort {
                            agent_id: Some(agent_down.clone()),
                            effort: None,
                        });
                        return;
                    }
                    let (origin_x, width) = *rail_area.peek();
                    let frac = effort_rail_fraction(e.global_location().x as f32 - origin_x, width);
                    dragging.set(true);
                    drag_frac.set(Some(frac));
                    let idx = effort_stop_index(frac, efforts_down.len());
                    if let Some(&effort) = efforts_down.get(idx)
                        && (effort != current_eff || was_unset)
                    {
                        core_down.invoke_or_log(Command::AgentSetEffort {
                            agent_id: Some(agent_down.clone()),
                            effort: Some(effort),
                        });
                    }
                })
                .on_global_pointer_move(move |e: Event<PointerEventData>| {
                    if !*dragging.peek() {
                        return;
                    }
                    let (origin_x, width) = *rail_area.peek();
                    let frac = effort_rail_fraction(e.global_location().x as f32 - origin_x, width);
                    drag_frac.set(Some(frac));
                    let idx = effort_stop_index(frac, efforts_move.len());
                    if let Some(&effort) = efforts_move.get(idx)
                        && (effort != current_eff || was_unset)
                    {
                        core_move.invoke_or_log(Command::AgentSetEffort {
                            agent_id: Some(agent_move.clone()),
                            effort: Some(effort),
                        });
                    }
                })
                .on_global_pointer_up(move |_| {
                    if *dragging.peek() {
                        let released_at = drag_frac.peek().unwrap_or(target_frac);
                        dragging.set(false);
                        anim_from_frac.set(released_at);
                        last_target_frac.set(target_frac);
                        drag_frac.set(None);
                        let next_seq = slide_seq.peek().wrapping_add(1);
                        slide_seq.set(next_seq);
                    }
                })
                .child(rail_el)
                .child(unfilled_stops)
                .child(fill_el)
                .maybe_child(focus_ring_el)
                .child(thumb_el),
        );

        KitTooltip::new("Double-click to go back to the agent's own setting")
            .fill_width(true)
            .enabled(!is_dragging)
            .child(slider_box)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Clickable full-width model card header (`zendia.css:2895-2975`).
#[derive(Clone, PartialEq)]
struct ModelTitleRow {
    effort_heading: Option<String>,
    model_name: String,
    on_open_list: EventHandler<()>,
}

impl Component for ModelTitleRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let title_a11y_id = use_a11y();
        let focus = use_focus(title_a11y_id);
        let fb = use_interactive_feedback(true);

        let hovered = *fb.hovered.read();
        let chevron_anim = use_animation_with_dependencies(&hovered, |conf, is_hov| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *is_hov { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::MODEL_HOVER)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let chev_p = chevron_anim.get().value().clamp(0.0, 1.0);
        let chev_x = 3.0 * chev_p;
        let chev_opacity = 0.55 + 0.45 * chev_p;

        let wash = theme.rgba(ThemeRole::Accent).with_alpha(0.14).to_color();
        let active_wash = theme.rgba(ThemeRole::Accent).with_alpha(0.22).to_color();
        let bg = fb.background(Color::TRANSPARENT, wash, active_wash);
        let name_color = if hovered {
            theme.color(ThemeRole::Text)
        } else {
            theme.color(ThemeRole::TextMuted)
        };

        let single = self.effort_heading.is_none();
        let on_open = self.on_open_list.clone();

        rect()
            .a11y_id(title_a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("{} · Change model", self.model_name))
            .width(Size::fill())
            .height(Size::px(if single { 36.0 } else { 48.0 }))
            .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
            .corner_radius(CornerRadius::new_all(CARD_INNER_RADIUS))
            .background(bg)
            .border(focus_ring_border(
                matches!(focus(), Focus::Keyboard),
                &theme,
                Border::new(),
            ))
            .main_align(Alignment::center())
            .cross_align(Alignment::start())
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_open.call(());
            })
            .maybe_child(self.effort_heading.as_ref().map(|heading| {
                RollingText::new(heading.clone())
                    .font_size(14.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(theme.color(ThemeRole::Text))
            }))
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(5.0)
                    .child(
                        label()
                            .text(self.model_name.clone())
                            .font_size(if single { 14.0 } else { 12.0 })
                            .font_weight(if single {
                                FontWeight::SEMI_BOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .color(if single {
                                theme.color(ThemeRole::Text)
                            } else {
                                name_color
                            })
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    )
                    .child(rect().offset_x(chev_x).opacity(chev_opacity).child(chevron_right_icon(
                        10.0,
                        if single {
                            theme.color(ThemeRole::Text)
                        } else {
                            name_color
                        },
                    ))),
            )
    }
}

/// Context window row (`zendia.css:3084-3110`): interactive submenu trigger or plain read-only label.
#[derive(Clone, PartialEq)]
struct ContextOptionRow {
    value_label: String,
    has_choices: bool,
    submenu_open: bool,
    on_enter: EventHandler<()>,
    on_leave: EventHandler<()>,
    on_toggle: EventHandler<()>,
    on_row_sized: EventHandler<f32>,
}

impl Component for ContextOptionRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(self.has_choices);

        let hovered = *fb.hovered.read();
        let chevron_anim = use_animation_with_dependencies(&hovered, |conf, is_hov| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *is_hov { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::MODEL_HOVER)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let chev_p = chevron_anim.get().value().clamp(0.0, 1.0);
        let chev_x = 3.0 * chev_p;
        let chev_opacity = 0.55 + 0.45 * chev_p;

        let active_bg = theme.rgba(ThemeRole::Accent).with_alpha(0.22).to_color();
        let row_bg = if !self.has_choices {
            Color::TRANSPARENT
        } else if self.submenu_open {
            active_bg
        } else {
            fb.background(Color::TRANSPARENT, active_bg, active_bg)
        };

        let on_enter = self.on_enter.clone();
        let on_leave = self.on_leave.clone();
        let on_toggle = self.on_toggle.clone();
        let on_sized = self.on_row_sized.clone();
        let mut fb_enter = fb.on_pointer_enter();
        let mut fb_leave = fb.on_pointer_leave();
        let has_choices = self.has_choices;

        let row = rect()
            .a11y_role(if has_choices {
                AccessibilityRole::Button
            } else {
                AccessibilityRole::GenericContainer
            })
            .a11y_alt(if has_choices {
                format!("Context window: {}", self.value_label)
            } else {
                format!("Context window {}", self.value_label)
            })
            .width(Size::fill())
            .height(Size::px(28.0))
            .margin(Gaps::new(2.0, 0.0, 0.0, 0.0))
            .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
            .corner_radius(CornerRadius::new_all(10.0))
            .background(row_bg)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .spacing(10.0)
            .cursor(if has_choices {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            })
            .on_sized(move |e: Event<SizedEventData>| {
                on_sized.call(e.area.min_y());
            })
            .on_pointer_enter(move |e: Event<PointerEventData>| {
                fb_enter(e);
                if has_choices {
                    on_enter.call(());
                }
            })
            .on_pointer_leave(move |e: Event<PointerEventData>| {
                fb_leave(e);
                if has_choices {
                    on_leave.call(());
                }
            })
            .on_press(move |e: Event<PressEventData>| {
                if has_choices {
                    e.stop_propagation();
                    on_toggle.call(());
                }
            })
            .child(
                rect().width(Size::flex(1.0)).child(
                    label()
                        .text("Context window")
                        .font_size(13.0)
                        .color(theme.color(ThemeRole::Text))
                        .max_lines(1),
                ),
            )
            .child(
                RollingText::new(self.value_label.clone())
                    .font_size(13.0)
                    .font_weight(FontWeight::NORMAL)
                    .color(theme.color(ThemeRole::TextMuted)),
            )
            .maybe_child(has_choices.then(|| {
                rect()
                    .offset_x(chev_x)
                    .opacity(chev_opacity)
                    .child(chevron_right_icon(10.0, theme.color(ThemeRole::TextMuted)))
            }));

        KitTooltip::new("Compacts the conversation when it reaches this size")
            .fill_width(true)
            .enabled(has_choices && !self.submenu_open)
            .child(row)
    }
}

/// One radio option inside the floating context window side submenu (`zendia.css:3125-3153`).
#[derive(Clone, PartialEq)]
struct ContextChoiceRow {
    choice: ContextChoice,
    is_selected: bool,
    is_active: bool,
    on_hover: EventHandler<()>,
    on_pick: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for ContextChoiceRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ContextChoiceRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let active_bg = theme.rgba(ThemeRole::Accent).with_alpha(0.22).to_color();
        let on_hover = self.on_hover.clone();
        let on_pick = self.on_pick.clone();

        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Set context window to {}", self.choice.label))
            .width(Size::fill())
            .height(Size::px(30.0))
            .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
            .corner_radius(CornerRadius::new_all(10.0))
            .background(if self.is_active { active_bg } else { Color::TRANSPARENT })
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .spacing(10.0)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| on_hover.call(()))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_pick.call(());
            })
            .child(
                rect().width(Size::flex(1.0)).child(
                    label()
                        .text(self.choice.label.clone())
                        .font_size(13.0)
                        .font_weight(if self.is_selected {
                            FontWeight::SEMI_BOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .color(if self.is_selected {
                            theme.color(ThemeRole::Accent)
                        } else {
                            theme.color(ThemeRole::Text)
                        })
                        .max_lines(1),
                ),
            )
            .maybe_child(self.choice.is_default.then(|| {
                label()
                    .text("Default")
                    .font_size(10.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(theme.color(ThemeRole::TextMuted))
            }))
            .maybe_child(
                self.is_selected
                    .then(|| Icon::new("check").size(12.0).role(ThemeRole::Accent)),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// `32×32 px` back button in the model list header (`zendia.css:3165-3188`).
#[derive(Clone, PartialEq)]
struct ModelBackButton {
    on_back: EventHandler<()>,
}

impl Component for ModelBackButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let wash = theme.rgba(ThemeRole::Accent).with_alpha(0.14).to_color();
        let active_wash = theme.rgba(ThemeRole::Accent).with_alpha(0.22).to_color();
        let on_back = self.on_back.clone();

        KitTooltip::new("Back").shortcut("Esc").child(
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Back")
                .width(Size::px(32.0))
                .height(Size::px(32.0))
                .corner_radius(CornerRadius::new_all(CARD_INNER_RADIUS))
                .background(fb.background(Color::TRANSPARENT, wash, active_wash))
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(fb.on_pointer_enter())
                .on_pointer_leave(fb.on_pointer_leave())
                .on_pointer_down(fb.on_pointer_down())
                .on_global_pointer_up(fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    on_back.call(());
                })
                .child(chevron_left_icon(12.0, theme.color(ThemeRole::Text))),
        )
    }
}

/// One row inside the searchable model list (`zendia.css:3222-3247`).
#[derive(Clone, PartialEq)]
struct ModelListRow {
    name: String,
    tag: Option<(String, bool)>,
    is_selected: bool,
    is_active: bool,
    on_hover: EventHandler<()>,
    on_pick: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for ModelListRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ModelListRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let active_bg = theme.rgba(ThemeRole::Accent).with_alpha(0.22).to_color();
        let on_hover = self.on_hover.clone();
        let on_pick = self.on_pick.clone();

        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.name.clone())
            .width(Size::fill())
            .height(Size::px(LIST_ROW_HEIGHT))
            .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
            .corner_radius(CornerRadius::new_all(10.0))
            .background(if self.is_active { active_bg } else { Color::TRANSPARENT })
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .cursor(CursorIcon::Pointer)
            .on_pointer_move(move |_| on_hover.call(()))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_pick.call(());
            })
            .child(
                rect().width(Size::flex(1.0)).overflow(Overflow::Clip).child(
                    label()
                        .text(self.name.clone())
                        .font_size(12.0)
                        .font_weight(if self.is_selected {
                            FontWeight::SEMI_BOLD
                        } else {
                            FontWeight::MEDIUM
                        })
                        .color(if self.is_selected {
                            theme.color(ThemeRole::Accent)
                        } else {
                            theme.color(ThemeRole::Text)
                        })
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                ),
            )
            .maybe_child(self.tag.as_ref().map(|(text, is_good)| {
                label()
                    .text(text.clone())
                    .font_size(10.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(if *is_good {
                        theme.color(ThemeRole::Success)
                    } else {
                        theme.color(ThemeRole::Warning)
                    })
                    .max_lines(1)
            }))
            .maybe_child(
                self.is_selected
                    .then(|| Icon::new("check").size(12.0).role(ThemeRole::Accent)),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
