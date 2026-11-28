/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Centered floating address bar and command palette (Actions mode), built on the generic [`PickerList`] kit
//! component and backed by `BrowserCore`'s result providers and command registry.

use crate::ui::chrome::address_bar::state::{
    AddressBarMode, compute_floating_bar_geometry, step_highlight, use_address_bar_state,
};
use crate::ui::chrome::sidebar::state::favicon_badge_colors;
use crate::ui::core_view::{CoreView, use_core};
use crate::ui::kit::{Icon, PickerItem, PickerList, PopupPresence, TextField, page_step_selectable};
use crate::ui::page::PageSurfaceContext;
use crate::ui::theme::{ResolvedScheme, ResolvedTheme, Rgba, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use zendia_browser_core::address_bar::{
    AddressBarFilter, AddressBarResult, AddressBarResultKind, ResolvedPaletteItem, query_address_bar_filtered,
    query_palette,
};
use zendia_browser_core::commands::Command;
use zendia_keymap::KeyTable;

const MAX_ADDRESS_BAR_RESULTS: usize = 6;
const MAX_EMPTY_SUGGESTIONS: usize = 5;

fn hex_rgb(c: Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// Compact Zendia mascot/identity emblem SVG shown at the leading edge of the floating address bar.
pub fn zendia_mascot_mini_svg(theme: &ResolvedTheme) -> Bytes {
    let accent = hex_rgb(theme.rgba(ThemeRole::Accent));
    let pink = hex_rgb(theme.palette.pink);
    let ink = hex_rgb(theme.palette.ink);
    let face = if theme.scheme.is_dark() { "#eef1f8" } else { "#ffffff" };
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24">
  <rect x="4" y="5" width="16" height="14" rx="4.5" fill="{face}" stroke="{ink}" stroke-width="1.7"/>
  <circle cx="12" cy="2.6" r="1.7" fill="{pink}"/>
  <path d="M12 4.3V5.2" stroke="{ink}" stroke-width="1.5" stroke-linecap="round"/>
  <circle cx="9.2" cy="11.2" r="1.6" fill="{ink}"/>
  <circle cx="14.8" cy="11.2" r="1.6" fill="{ink}"/>
  <rect x="8.2" y="14.5" width="7.6" height="2" rx="1" fill="{accent}"/>
</svg>"##
    );
    Bytes::from(svg.into_bytes())
}

fn address_bar_result_to_picker_item(
    result: &AddressBarResult,
    theme: &ResolvedTheme,
    query_is_empty: bool,
) -> PickerItem {
    let is_heuristic = !query_is_empty
        && matches!(
            result.kind,
            AddressBarResultKind::Search { .. } | AddressBarResultKind::NavigateUrl { .. }
        );
    let mut item = PickerItem::new(result.id.clone(), result.title.clone()).accent_highlight(is_heuristic);
    if let Some(sub) = &result.subtitle {
        item = item.subtitle(sub.clone());
    }
    if let Some(icon) = &result.icon {
        item = item.icon(icon.clone());
    }
    match &result.kind {
        AddressBarResultKind::SwitchToTab { .. } | AddressBarResultKind::Bookmark { .. } => {
            let (bg, fg) = favicon_badge_colors(result.icon.as_deref(), theme);
            item = item.icon_badge(bg, fg);
        }
        _ => {}
    }
    if let Some(badge) = &result.badge_text {
        item = item.action_chip(badge.clone());
    }
    if let Some((r, g, b)) = result.badge_accent {
        item = item.action_chip_color(Color::from_rgb(r, g, b));
    }
    if let Some(shortcut) = &result.shortcut_hint {
        item = item.key_hint(shortcut.clone());
    }
    item
}

fn palette_item_to_picker_item(entry: &ResolvedPaletteItem) -> PickerItem {
    let mut item = PickerItem::new(entry.id.clone(), entry.title.clone()).accent_highlight(false);
    if let Some(icon) = &entry.icon {
        item = item.icon(icon.clone());
    }
    if let Some(badge) = &entry.badge_text {
        item = item.action_chip(badge.clone());
    }
    if let Some((r, g, b)) = entry.badge_accent {
        item = item.action_chip_color(Color::from_rgb(r, g, b));
    }
    if let Some(shortcut) = &entry.shortcut_hint {
        item = item.key_hint(shortcut.clone());
    }
    item
}

fn execute_address_bar_result(core: &CoreView, result: &AddressBarResult, new_tab: bool) {
    match &result.kind {
        AddressBarResultKind::Search { query, .. } => {
            core.invoke_or_log(Command::AddressBarSubmit {
                input: query.clone(),
                new_tab,
            });
        }
        AddressBarResultKind::NavigateUrl { url, .. }
        | AddressBarResultKind::History { url, .. }
        | AddressBarResultKind::Bookmark { url, .. } => {
            core.invoke_or_log(Command::AddressBarSubmit {
                input: url.clone(),
                new_tab,
            });
        }
        AddressBarResultKind::SwitchToTab { tab_id, .. } => {
            core.invoke_or_log(Command::AddressBarSwitchToTab { tab_id: *tab_id });
        }
        AddressBarResultKind::Folder {
            folder_id, space_id, ..
        } => {
            core.invoke_or_log(Command::SpaceSwitch { space_id: *space_id });
            let is_collapsed = core
                .model()
                .peek()
                .folder(*folder_id)
                .is_some_and(|folder| folder.collapsed);
            if is_collapsed {
                core.invoke_or_log(Command::FolderToggleCollapsed { folder_id: *folder_id });
            }
        }
        AddressBarResultKind::Action {
            command, command_args, ..
        } => {
            if let Ok(parsed) = Command::from_name_and_args(command, command_args) {
                core.invoke_or_log(parsed);
            }
        }
    }
}

fn execute_palette_item(core: &CoreView, entry: &ResolvedPaletteItem) {
    match Command::from_name_and_args(&entry.command, &entry.command_args) {
        Ok(parsed) => {
            core.invoke_or_log(parsed);
        }
        Err(error) => {
            eprintln!("zendia: palette command `{}` failed to parse: {error}", entry.command);
        }
    }
}

/// Window-root overlay rendering the centered floating address bar / command palette over a dimmed page backdrop.
#[derive(Clone, PartialEq)]
pub struct FloatingAddressBarOverlay;

impl Component for FloatingAddressBarOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let state = use_address_bar_state();
        let page_ctx = try_consume_context::<PageSurfaceContext>();

        let is_open = *state.open.read();
        let mode = *state.mode.read();
        let open_seq = *state.open_seq.read();

        let root_size = *Platform::get().current_window().root_size.read();
        let window_w = root_size.width.max(480.0);
        let window_h = root_size.height.max(360.0);
        let (bar_width, base_top) = compute_floating_bar_geometry(window_w, window_h);
        let left = ((window_w - bar_width) * 0.5).max(16.0);

        let _ = theme;
        let on_close = {
            let page_ctx = page_ctx.clone();
            let cancel_core = core.clone();
            move || {
                state.close();
                cancel_core.invoke_or_log(Command::Vim(
                    zendia_browser_core::vim_keys::VimCommand::AddressBarCancelled,
                ));
                if let Some(ctx) = &page_ctx {
                    ctx.request_focus_active_surface();
                }
            }
        };
        let on_submit_close = {
            let page_ctx = page_ctx.clone();
            move || {
                state.close_after_submit();
                if let Some(ctx) = &page_ctx {
                    ctx.request_focus_active_surface();
                }
            }
        };
        let on_close_backdrop = on_close.clone();
        let on_close_card = on_close.clone();
        let on_close_esc = on_close;

        let _ = core;

        let select_all_on_open = is_open
            && matches!(mode, AddressBarMode::Normal { new_tab: false })
            && !state.query.peek().trim().is_empty();

        PopupPresence::anchored(is_open, (left, base_top.max(24.0)))
            .open_seq(open_seq)
            .on_escape(move |_| on_close_esc())
            .on_backdrop_mouse_down(move |_| on_close_backdrop())
            .child(
                FloatingAddressBarCard::new(state.query, state.highlighted, mode)
                    .filter(state.filter)
                    .width(bar_width)
                    .input_a11y_id(state.input_a11y_id)
                    .auto_focus(is_open)
                    .select_all_on_open(select_all_on_open)
                    .select_all_seq(open_seq)
                    .on_enter_actions(move |_| state.enter_actions_mode())
                    .on_exit_actions(move |_| state.exit_actions_mode())
                    .on_submit_close(move |_| on_submit_close())
                    .on_close(move |_| on_close_card())
                    .key(open_seq),
            )
    }
}

/// Reusable floating address bar and command palette card, rendered both in [`FloatingAddressBarOverlay`] and inline
/// inside the UI gallery.
#[derive(Clone, PartialEq)]
pub struct FloatingAddressBarCard {
    query: State<String>,
    highlighted: State<Option<usize>>,
    mode: AddressBarMode,
    filter: Option<State<AddressBarFilter>>,
    width: f32,
    input_a11y_id: Option<AccessibilityId>,
    auto_focus: bool,
    select_all_on_open: bool,
    select_all_seq: u64,
    on_enter_actions: Option<EventHandler<()>>,
    on_exit_actions: Option<EventHandler<()>>,
    on_submit_close: Option<EventHandler<()>>,
    on_close: Option<EventHandler<()>>,
    key: DiffKey,
}

impl KeyExt for FloatingAddressBarCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl FloatingAddressBarCard {
    pub fn new(query: State<String>, highlighted: State<Option<usize>>, mode: AddressBarMode) -> Self {
        Self {
            query,
            highlighted,
            mode,
            filter: None,
            width: 750.0,
            input_a11y_id: None,
            auto_focus: false,
            select_all_on_open: false,
            select_all_seq: 0,
            on_enter_actions: None,
            on_exit_actions: None,
            on_submit_close: None,
            on_close: None,
            key: DiffKey::None,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn filter(mut self, filter: State<AddressBarFilter>) -> Self {
        self.filter = Some(filter);
        self
    }

    pub fn input_a11y_id(mut self, id: AccessibilityId) -> Self {
        self.input_a11y_id = Some(id);
        self
    }

    pub fn auto_focus(mut self, auto_focus: bool) -> Self {
        self.auto_focus = auto_focus;
        self
    }

    pub fn select_all_on_open(mut self, select_all: bool) -> Self {
        self.select_all_on_open = select_all;
        self
    }

    pub fn select_all_seq(mut self, seq: u64) -> Self {
        self.select_all_seq = seq;
        self
    }

    pub fn on_enter_actions(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_enter_actions = Some(handler.into());
        self
    }

    pub fn on_exit_actions(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_exit_actions = Some(handler.into());
        self
    }

    pub fn on_submit_close(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_submit_close = Some(handler.into());
        self
    }

    pub fn on_close(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_close = Some(handler.into());
        self
    }
}

impl Component for FloatingAddressBarCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let key_table = use_hook(KeyTable::standard);
        let fallback_a11y_id = use_hook(AccessibilityId::new_unique);
        let input_a11y_id = self.input_a11y_id.unwrap_or(fallback_a11y_id);

        let is_actions = matches!(self.mode, AddressBarMode::Actions);
        let new_tab = matches!(self.mode, AddressBarMode::Normal { new_tab: true });

        // Search-mode scale bounce [1.0, 0.98, 1.0] over 250ms and 1s expanding accent glow (matching Zen).
        let mode_bounce = use_animation_with_dependencies(&is_actions, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(motion::URLBAR_MODE_BOUNCE)
                .function(Function::Linear)
        });
        let mode_glow = use_animation_with_dependencies(&is_actions, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(motion::URLBAR_MODE_GLOW)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let bounce_t = mode_bounce.get().value().clamp(0.0, 1.0);
        let bounce_scale = 1.0 - 0.02 * (4.0 * bounce_t * (1.0 - bounce_t));
        let glow_t = mode_glow.get().value().clamp(0.0, 1.0);
        let pulse_strength = (1.0 - bounce_t).clamp(0.0, 1.0);

        let query_text = self.query.read().clone();
        let current_filter = self.filter.map(|f| *f.read()).unwrap_or(AddressBarFilter::All);
        let mut last_query = use_state(|| query_text.clone());

        let (address_results, palette_results, filter_chip_label) = {
            let model = core.model().read();
            let chip_label = (!is_actions)
                .then(|| current_filter.chip_label(&model.search_engine))
                .flatten();
            if is_actions {
                let commands = core.commands();
                (
                    Vec::new(),
                    query_palette(&model, &commands, &query_text, Some(&key_table)),
                    None,
                )
            } else {
                let cap = if query_text.trim().is_empty() && current_filter == AddressBarFilter::All {
                    MAX_EMPTY_SUGGESTIONS
                } else {
                    MAX_ADDRESS_BAR_RESULTS
                };
                (
                    query_address_bar_filtered(&model, &query_text, current_filter, Some(&key_table), cap),
                    Vec::new(),
                    chip_label,
                )
            }
        };

        let result_count = if is_actions {
            palette_results.len()
        } else {
            address_results.len()
        };

        let mut highlighted = self.highlighted;
        if *last_query.peek() != query_text {
            last_query.set(query_text.clone());
            let next_highlight = if result_count == 0 {
                None
            } else if is_actions || !query_text.trim().is_empty() {
                Some(0)
            } else {
                None
            };
            highlighted.set(next_highlight);
        }

        let current_highlighted = highlighted.read().filter(|&idx| idx < result_count);

        let query_is_empty = query_text.trim().is_empty();
        let picker_items: Vec<PickerItem> = if is_actions {
            palette_results.iter().map(palette_item_to_picker_item).collect()
        } else {
            address_results
                .iter()
                .map(|r| address_bar_result_to_picker_item(r, &theme, query_is_empty))
                .collect()
        };

        let run_index = {
            let core = core.clone();
            let address_results = address_results.clone();
            let palette_results = palette_results.clone();
            let on_close = self.on_submit_close.clone().or_else(|| self.on_close.clone());
            move |idx: usize| {
                if is_actions {
                    if let Some(item) = palette_results.get(idx) {
                        execute_palette_item(&core, item);
                    }
                } else if let Some(result) = address_results.get(idx) {
                    execute_address_bar_result(&core, result, new_tab);
                }
                if let Some(close) = &on_close {
                    close.call(());
                }
            }
        };

        let run_submit_or_highlighted = {
            let core = core.clone();
            let run_index = run_index.clone();
            let on_close = self.on_submit_close.clone().or_else(|| self.on_close.clone());
            let query = self.query;
            move || {
                let chosen = (*highlighted.peek()).filter(|&idx| idx < result_count);
                if let Some(idx) = chosen {
                    run_index(idx);
                } else if !is_actions {
                    let typed = query.peek().clone();
                    if !typed.trim().is_empty() {
                        core.invoke_or_log(Command::AddressBarSubmit { input: typed, new_tab });
                        if let Some(close) = &on_close {
                            close.call(());
                        }
                    }
                } else if result_count > 0 {
                    run_index(0);
                }
            }
        };

        let query_state = self.query;
        let filter_state_opt = self.filter;
        let on_enter_actions = self.on_enter_actions.clone();
        let on_exit_actions = self.on_exit_actions.clone();
        let on_close_key = self.on_close.clone();
        let run_on_enter_key = run_submit_or_highlighted.clone();

        let on_pre_key_down = move |e: Event<KeyboardEventData>| {
            let ctrl_or_alt = e.modifiers.contains(Modifiers::CONTROL) || e.modifiers.contains(Modifiers::ALT);
            match &e.key {
                Key::Named(NamedKey::Escape) => {
                    e.stop_propagation();
                    e.prevent_default();
                    if let Some(close) = &on_close_key {
                        close.call(());
                    }
                    false
                }
                Key::Named(NamedKey::ArrowDown) => {
                    e.stop_propagation();
                    e.prevent_default();
                    let next = step_highlight(*highlighted.peek(), result_count, 1);
                    highlighted.set(next);
                    false
                }
                Key::Named(NamedKey::ArrowUp) => {
                    e.stop_propagation();
                    e.prevent_default();
                    let next = step_highlight(*highlighted.peek(), result_count, -1);
                    highlighted.set(next);
                    false
                }
                Key::Named(NamedKey::PageDown) => {
                    e.stop_propagation();
                    e.prevent_default();
                    let next = page_step_selectable(result_count, *highlighted.peek(), 5, true, |_| true);
                    highlighted.set(next);
                    false
                }
                Key::Named(NamedKey::PageUp) => {
                    e.stop_propagation();
                    e.prevent_default();
                    let next = page_step_selectable(result_count, *highlighted.peek(), 5, false, |_| true);
                    highlighted.set(next);
                    false
                }
                Key::Named(NamedKey::Tab) if !ctrl_or_alt => {
                    e.stop_propagation();
                    e.prevent_default();
                    let shift = e.modifiers.contains(Modifiers::SHIFT);
                    if !is_actions && !shift && query_state.peek().is_empty() {
                        if let Some(handler) = &on_enter_actions {
                            handler.call(());
                        }
                    } else {
                        let delta = if shift { -1 } else { 1 };
                        let next = step_highlight(*highlighted.peek(), result_count, delta);
                        highlighted.set(next);
                    }
                    false
                }
                Key::Character(ch)
                    if e.modifiers.contains(Modifiers::CONTROL) && !e.modifiers.contains(Modifiers::ALT) =>
                {
                    let lower = ch.to_ascii_lowercase();
                    if lower == "n" || e.code == Code::KeyN {
                        e.stop_propagation();
                        e.prevent_default();
                        let next = step_highlight(*highlighted.peek(), result_count, 1);
                        highlighted.set(next);
                        false
                    } else if lower == "p" || e.code == Code::KeyP {
                        e.stop_propagation();
                        e.prevent_default();
                        let next = step_highlight(*highlighted.peek(), result_count, -1);
                        highlighted.set(next);
                        false
                    } else {
                        true
                    }
                }
                Key::Named(NamedKey::Backspace) if query_state.peek().is_empty() => {
                    if is_actions {
                        e.stop_propagation();
                        e.prevent_default();
                        if let Some(handler) = &on_exit_actions {
                            handler.call(());
                        }
                        false
                    } else if let Some(mut f_state) = filter_state_opt
                        && *f_state.peek() != AddressBarFilter::All
                    {
                        e.stop_propagation();
                        e.prevent_default();
                        f_state.set(AddressBarFilter::All);
                        highlighted.set(None);
                        false
                    } else {
                        e.stop_propagation();
                        true
                    }
                }
                Key::Named(NamedKey::Enter) => {
                    e.stop_propagation();
                    e.prevent_default();
                    run_on_enter_key();
                    false
                }
                _ => {
                    e.stop_propagation();
                    true
                }
            }
        };

        // Matches Zen's `#urlbar[breakout-extend] .urlbar-background`: opaque surface with `backdrop-filter: none`
        // by default (`zen-omnibox.css:268-281`), avoiding multi-hundred-millisecond full-screen CPU blur passes.
        let card_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_rgb(34, 34, 36),
            ResolvedScheme::Light => Color::from_rgb(236, 236, 239),
        };
        let base_border = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(115, 115, 122, 0.45),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.18),
        };
        let border_color = if pulse_strength > 0.01 {
            base_border
                .mix(theme.rgba(ThemeRole::Accent), pulse_strength * 0.75)
                .to_color()
        } else {
            base_border.to_color()
        };
        let divider_color = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.09).to_color(),
        };
        let text_color = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(240, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(230, 22, 24, 34),
        };
        let placeholder_color = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(128, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(130, 60, 64, 78),
        };
        let shadow = match theme.scheme {
            ResolvedScheme::Dark => Shadow::new()
                .x(0.0)
                .y(18.0)
                .blur(48.0)
                .spread(-6.0)
                .color(Color::from_argb(190, 0, 0, 0)),
            ResolvedScheme::Light => Shadow::new()
                .x(0.0)
                .y(18.0)
                .blur(44.0)
                .spread(-6.0)
                .color(Color::from_argb(95, 0, 0, 0)),
        };
        let glow_shadow = (is_actions && glow_t > 0.001 && glow_t < 0.999).then(|| {
            let accent = theme.rgba(ThemeRole::Accent);
            Shadow::new()
                .x(0.0)
                .y(0.0)
                .blur(12.0 + 48.0 * glow_t)
                .color(accent.with_alpha((1.0 - glow_t) * 0.45).to_color())
        });

        let mascot_bytes = zendia_mascot_mini_svg(&theme);
        let mascot_key = if theme.scheme.is_dark() {
            "zendia-urlbar-mascot-dark"
        } else {
            "zendia-urlbar-mascot-light"
        };

        let actions_chip_bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(78, 82, 114, 0.96).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(82, 108, 178, 0.96).to_color(),
        };

        let mode_chip = if is_actions {
            let on_exit = self.on_exit_actions.clone();
            Some(
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("Actions mode (press Backspace on empty input to exit)")
                    .height(Size::px(24.0))
                    .padding(Gaps::new(0.0, 9.0, 0.0, 9.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(actions_chip_bg)
                    .main_align(Alignment::center())
                    .cross_align(Alignment::center())
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        if let Some(handler) = &on_exit {
                            handler.call(());
                        }
                    })
                    .child(
                        label()
                            .text("Actions")
                            .font_size(12.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(Color::WHITE),
                    ),
            )
        } else if let Some(chip_text) = filter_chip_label {
            let filter_opt = self.filter;
            Some(
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt(format!("{chip_text} filter (press Backspace on empty input to clear)"))
                    .height(Size::px(24.0))
                    .padding(Gaps::new(0.0, 9.0, 0.0, 9.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(actions_chip_bg)
                    .main_align(Alignment::center())
                    .cross_align(Alignment::center())
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        if let Some(mut f) = filter_opt {
                            f.set(AddressBarFilter::All);
                            highlighted.set(None);
                        }
                    })
                    .child(
                        label()
                            .text(chip_text)
                            .font_size(12.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(Color::WHITE),
                    ),
            )
        } else {
            None
        };

        let show_results = !picker_items.is_empty() || is_actions || current_filter != AddressBarFilter::All;
        let run_on_select = run_index;
        let run_on_submit = run_submit_or_highlighted;

        let input_row = rect()
            .width(Size::fill())
            .height(Size::px(62.0))
            .padding(Gaps::new(0.0, 16.0, 0.0, 16.0))
            .spacing(10.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .child(
                SvgViewer::new((mascot_key, mascot_bytes))
                    .width(Size::px(18.0))
                    .height(Size::px(18.0))
                    .show_loader(false),
            )
            .child(Icon::new("search-glass").size(15.0).role(ThemeRole::TextMuted))
            .maybe_child(mode_chip)
            .child(
                rect().width(Size::flex(1.0)).font_size(17.0).child(
                    TextField::new(self.query)
                        .a11y_id(input_a11y_id)
                        .auto_focus(self.auto_focus)
                        .select_all_on_open(self.select_all_on_open)
                        .select_all_seq(self.select_all_seq)
                        .placeholder("Search...")
                        .width(Size::fill())
                        .color(text_color)
                        .placeholder_color(placeholder_color)
                        .padding(Gaps::new(0.0, 2.0, 0.0, 2.0))
                        .on_pre_key_down(on_pre_key_down)
                        .on_submit(move |_: String| {
                            run_on_submit();
                        }),
                ),
            );

        let a11y_title = if is_actions {
            "Command Palette (Actions mode)"
        } else if new_tab {
            "New Tab Address Bar"
        } else {
            "Address Bar"
        };

        let mut card = rect()
            .a11y_role(AccessibilityRole::Dialog)
            .a11y_alt(a11y_title)
            .width(Size::px(self.width))
            .scale(bounce_scale)
            .corner_radius(CornerRadius::new_all(15.0))
            .overflow(Overflow::Clip)
            .background(card_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_color),
            )
            .shadow(shadow)
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .child(input_row);
        if let Some(glow) = glow_shadow {
            card = card.shadow(glow);
        }

        if show_results {
            card = card
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(1.0))
                        .background(divider_color),
                )
                .child(
                    PickerList::new(picker_items)
                        .highlighted(current_highlighted)
                        .highlight_query(query_text.clone())
                        .max_height(244.0)
                        .empty_message("No matching actions")
                        .on_select(move |idx: usize| {
                            run_on_select(idx);
                        }),
                );
        }

        card
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
