/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Native Freya settings window: sidebar of groups, live search box, Zen and Zendia groups, plugins, extensions,
//! background services, and secondary OS window launcher (`WinitPlatformExt::launch_window`).

pub mod controls;
pub mod look_and_feel;
pub mod plugins_and_extensions;
pub mod search_results;
pub mod sidebar;
pub mod tabs_and_spaces;
pub mod zendia_groups;

use crate::ui::core_view::{use_core, use_provide_core_with_observer};
use crate::ui::kit::{Icon, IconButton, TextField, try_use_text_focus, use_provide_text_focus};
use crate::ui::settings::controls::SettingsPalette;
use crate::ui::settings::look_and_feel::LookAndFeelGroup;
use crate::ui::settings::plugins_and_extensions::{ExtensionsAndServicesGroup, PluginsGroup};
use crate::ui::settings::search_results::SettingsSearchResults;
use crate::ui::settings::sidebar::SettingsSidebar;
use crate::ui::settings::tabs_and_spaces::{GlanceGroup, SpacesGroup, TabManagementGroup};
use crate::ui::settings::zendia_groups::{AgentsSettingsGroup, VimSettingsGroup, WebChatSettingsGroup};
use crate::ui::theme::{CHROME_OVERLAY_LEVEL, ThemeRole, use_provide_theme, use_theme};
use crate::ui::vim::mascot::{MASCOT_FULL_SIZE, MASCOT_REST_SCALE, VimMascot};
use crate::ui::vim::statusline::STATUSLINE_CORNER_GAP;
use freya::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zendia_browser_core::BrowserCore;
use zendia_browser_core::commands::Command;
use zendia_browser_core::input_state::StatuslineShow;
use zendia_browser_core::model::InputMode;
use zendia_browser_core::settings::SettingsGroupId;

/// Context exposing active `s` jump-label hints inside [`SettingsView`] (`PG-01`).
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct SettingsVimJumpContext {
    pub active: State<bool>,
}

const SETTINGS_GROUP_HINT_KEYS: &[(SettingsGroupId, &str)] = &[
    (SettingsGroupId::LookAndFeel, "s"),
    (SettingsGroupId::TabManagement, "d"),
    (SettingsGroupId::Spaces, "f"),
    (SettingsGroupId::Glance, "g"),
    (SettingsGroupId::Vim, "h"),
    (SettingsGroupId::Agents, "l"),
    (SettingsGroupId::WebChat, "q"),
    (SettingsGroupId::Plugins, "w"),
    (SettingsGroupId::Extensions, "e"),
];

pub(crate) fn settings_vim_hint_for_group(group: SettingsGroupId) -> Option<String> {
    let ctx = try_consume_context::<SettingsVimJumpContext>()?;
    if !*ctx.active.read() {
        return None;
    }
    SETTINGS_GROUP_HINT_KEYS
        .iter()
        .find_map(|(g, key)| (*g == group).then(|| (*key).to_string()))
}

pub(crate) fn settings_vim_hint_for_link(link: &str) -> Option<String> {
    let ctx = try_consume_context::<SettingsVimJumpContext>()?;
    if !*ctx.active.read() {
        return None;
    }
    match link {
        "ladybird" => Some("r".to_string()),
        "about" => Some("u".to_string()),
        _ => None,
    }
}

/// Shared handle coordinating a second OS settings window opened from the main browser window.
#[derive(Clone)]
pub struct SettingsWindowHost {
    window_id: Arc<Mutex<Option<freya::winit::window::WindowId>>>,
    group_tx: Arc<Mutex<Option<async_channel::Sender<SettingsGroupId>>>>,
}

impl Default for SettingsWindowHost {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsWindowHost {
    pub fn new() -> Self {
        Self {
            window_id: Arc::new(Mutex::new(None)),
            group_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// Opens the native Settings window as a real second OS window via Freya 0.5.4's `WinitPlatformExt::launch_window`,
    /// or focuses the already-open Settings window and switches it to `group`.
    pub fn open_os_window(&self, core: Arc<dyn BrowserCore>, group: Option<SettingsGroupId>) {
        let target_group = group.unwrap_or(SettingsGroupId::LookAndFeel);
        let existing = *self.window_id.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(win_id) = existing {
            if let Some(tx) = self.group_tx.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                let _ = tx.try_send(target_group);
            }
            Platform::get().focus_window(win_id);
            return;
        }

        let (tx, rx) = async_channel::unbounded();
        *self.group_tx.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);

        let window_id_slot = Arc::clone(&self.window_id);
        let window_id_on_close = Arc::clone(&self.window_id);
        let group_tx_on_close = Arc::clone(&self.group_tx);

        spawn_in_window(async move {
            let app = SettingsOsWindowApp {
                core,
                initial_group: target_group,
                group_rx: rx,
            };
            let config = WindowConfig::new_app(app)
                .with_title("Zendia Settings")
                .with_size(1080.0, 760.0)
                .with_min_size(760.0, 520.0)
                .with_on_close(move |_, _| {
                    *window_id_on_close.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    *group_tx_on_close.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    CloseDecision::Close
                });
            let launched_id = Platform::get().launch_window(config).await;
            *window_id_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(launched_id);
        });
    }

    /// Closes the secondary Settings OS window if it is currently open.
    pub fn close_os_window(&self) {
        let existing = self.window_id.lock().unwrap_or_else(|e| e.into_inner()).take();
        *self.group_tx.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if let Some(win_id) = existing {
            Platform::get().close_window(win_id);
        }
    }
}

/// Installs [`SettingsWindowHost`] into Freya context for the browser window.
pub fn use_provide_settings_host() -> SettingsWindowHost {
    let host = use_hook(SettingsWindowHost::new);
    use_provide_context({
        let host = host.clone();
        move || host
    })
}

/// Standalone Freya `App` mounted inside the secondary OS Settings window.
struct SettingsOsWindowApp {
    core: Arc<dyn BrowserCore>,
    initial_group: SettingsGroupId,
    group_rx: async_channel::Receiver<SettingsGroupId>,
}

impl App for SettingsOsWindowApp {
    fn render(&self) -> impl IntoElement {
        let core = use_provide_core_with_observer(Arc::clone(&self.core), |_| {});
        let _theme_state = use_provide_theme(&core);
        let _text_focus = use_provide_text_focus();
        let mut active_group = use_state(|| self.initial_group);
        let search_query = use_state(String::new);

        use_hook({
            let rx = self.group_rx.clone();
            move || {
                spawn_in_window(async move {
                    while let Ok(next_group) = rx.recv().await {
                        active_group.set_if_modified(next_group);
                    }
                });
            }
        });

        SettingsView::new(active_group, search_query).closable_os_window(true)
    }
}

/// Complete native Settings UI surface usable both inside its own OS window and embedded in `zendia-gallery`.
#[derive(Clone, PartialEq)]
pub struct SettingsView {
    pub active_group: State<SettingsGroupId>,
    pub search_query: State<String>,
    pub closable_os_window: bool,
    key: DiffKey,
}

impl KeyExt for SettingsView {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl SettingsView {
    pub fn new(active_group: State<SettingsGroupId>, search_query: State<String>) -> Self {
        Self {
            active_group,
            search_query,
            closable_os_window: false,
            key: DiffKey::None,
        }
    }

    pub fn closable_os_window(mut self, closable: bool) -> Self {
        self.closable_os_window = closable;
        self
    }
}

impl Component for SettingsView {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let text_focus = try_use_text_focus();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let search_a11y_id = use_hook(AccessibilityId::new_unique);
        let search_focus = use_focus(search_a11y_id);
        let mut search_focus_seq = use_state(|| 0u64);
        let mut view_width = use_state(|| 1080.0_f32);
        let mut view_height = use_state(|| 760.0_f32);
        let mut content_height = use_state(|| 1200.0_f32);
        let mut scroll_controller = use_scroll_controller(ScrollConfig::default);
        let mut active_group = self.active_group;
        let mut search_query = self.search_query;

        // Native Settings vim state (`PG-01`): `s` jump labels, `gg`/`gt`/`gT` prefix, count buffer, and statusline.
        let jump_active = use_state(|| false);
        use_provide_context(move || SettingsVimJumpContext { active: jump_active });
        let mut pending_g = use_state(|| false);
        let mut count_buf = use_state(String::new);
        let sl_awake = use_state(|| true);
        let sl_wake_seq = use_state(|| 0u64);

        let (vim_enabled, statusline_settings) = {
            let model = core.model().read();
            (model.vim_mode_enabled, model.statusline.clone())
        };

        let current_group = *active_group.read();
        let current_query = search_query.read().clone();
        let is_searching = !current_query.trim().is_empty();
        let closable = self.closable_os_window;

        // Reset scroll to top whenever the active group or search-active state changes (`SET-04`).
        let mut last_scroll_key = use_state(|| (current_group, is_searching));
        if *last_scroll_key.peek() != (current_group, is_searching) {
            last_scroll_key.set((current_group, is_searching));
            scroll_controller.scroll_to_y(0);
        }

        let compact = *view_width.read() < 880.0;
        let search_focused = search_focus() != Focus::Not;
        let any_field_focused = search_focused || text_focus.is_some_and(|tf| tf.is_any_focused());

        let wake_statusline = move || {
            let mut seq_state = sl_wake_seq;
            let mut awake_state = sl_awake;
            let seq = seq_state.peek().wrapping_add(1);
            seq_state.set(seq);
            awake_state.set_if_modified(true);
            spawn(async move {
                timer(Duration::from_millis(2000)).await;
                if *seq_state.peek() == seq {
                    awake_state.set_if_modified(false);
                }
            });
        };

        let core_keys = core.clone();
        let on_global_key_down = move |e: Event<KeyboardEventData>| {
            if e.modifiers.intersects(Modifiers::CONTROL | Modifiers::META) && e.code == Code::KeyF {
                e.stop_propagation();
                e.prevent_default();
                let mut ja = jump_active;
                ja.set_if_modified(false);
                pending_g.set_if_modified(false);
                count_buf.set_if_modified(String::new());
                let next = search_focus_seq.peek().wrapping_add(1);
                search_focus_seq.set(next);
                search_a11y_id.request_focus();
                wake_statusline();
                return;
            }

            let field_focused_now = search_a11y_id.is_focused() || text_focus.is_some_and(|tf| tf.is_any_focused());

            if matches!(e.key, Key::Named(NamedKey::Escape)) {
                e.stop_propagation();
                let mut ja = jump_active;
                if *ja.peek() {
                    ja.set(false);
                    wake_statusline();
                    return;
                }
                if *pending_g.peek() || !count_buf.peek().is_empty() {
                    pending_g.set_if_modified(false);
                    count_buf.set_if_modified(String::new());
                    wake_statusline();
                    return;
                }
                if field_focused_now {
                    search_a11y_id.request_unfocus();
                    if let Some(tf) = text_focus {
                        tf.blur_all();
                    }
                    wake_statusline();
                    return;
                }
                if !search_query.peek().is_empty() {
                    search_query.set(String::new());
                } else if closable {
                    Platform::get().close_window(Platform::window_id());
                }
                return;
            }

            if !vim_enabled || field_focused_now {
                return;
            }

            // Active `s` jump labels inside Settings (`PG-01`).
            if *jump_active.peek() {
                if crate::input::key_stroke_from_event(&e.key, e.code, e.modifiers).is_none() {
                    return;
                }
                e.stop_propagation();
                e.prevent_default();
                let mut ja = jump_active;
                ja.set(false);
                wake_statusline();
                if !e
                    .modifiers
                    .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META)
                    && let Key::Character(ch) = &e.key
                {
                    let lower = ch.to_ascii_lowercase();
                    if lower == "a" {
                        let next = search_focus_seq.peek().wrapping_add(1);
                        search_focus_seq.set(next);
                        search_a11y_id.request_focus();
                    } else if lower == "r" {
                        core_keys.invoke_or_log(Command::TabNew {
                            url: Some("about:settings".to_string()),
                            after: None,
                            activate: true,
                        });
                    } else if lower == "u" {
                        core_keys.invoke_or_log(Command::AppAbout);
                    } else if let Some(&(grp, _)) = SETTINGS_GROUP_HINT_KEYS.iter().find(|(_, k)| *k == lower) {
                        search_query.set(String::new());
                        active_group.set(grp);
                    }
                }
                return;
            }

            let ctrl_only = e.modifiers == Modifiers::CONTROL;
            let plain = !e
                .modifiers
                .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META);

            let take_count = |buf: &mut State<String>| -> i32 {
                let raw = buf.peek().clone();
                buf.set_if_modified(String::new());
                raw.parse::<i32>().unwrap_or(1).clamp(1, 999)
            };
            let scroll_by = |sc: &mut ScrollController, dy: i32, view_h: f32, content_h: f32| {
                let (_, raw_y): (i32, i32) = (*sc).into();
                let cur_y = (-raw_y).max(0);
                let max_y = (content_h - view_h).max(0.0).round() as i32;
                let next_y = (cur_y + dy).clamp(0, max_y);
                let _ = sc.scroll_to_y(-next_y);
            };
            let step_group = |delta: isize, grp_state: &mut State<SettingsGroupId>, sq: &mut State<String>| {
                let all = SettingsGroupId::ALL;
                let idx = all.iter().position(|g| *g == *grp_state.peek()).unwrap_or(0) as isize;
                let next = (idx + delta).rem_euclid(all.len() as isize) as usize;
                sq.set(String::new());
                grp_state.set(all[next]);
            };

            if ctrl_only {
                let vh = (*view_height.peek()).max(240.0);
                let ch = (*content_height.peek()).max(vh);
                let is_key = |code: Code, c: &str| {
                    e.code == code || matches!(&e.key, Key::Character(k) if k.eq_ignore_ascii_case(c))
                };
                if is_key(Code::KeyD, "d") {
                    e.stop_propagation();
                    e.prevent_default();
                    pending_g.set_if_modified(false);
                    let n = take_count(&mut count_buf);
                    scroll_by(&mut scroll_controller, ((vh * 0.5) as i32) * n, vh, ch);
                    wake_statusline();
                } else if is_key(Code::KeyU, "u") {
                    e.stop_propagation();
                    e.prevent_default();
                    pending_g.set_if_modified(false);
                    let n = take_count(&mut count_buf);
                    scroll_by(&mut scroll_controller, -((vh * 0.5) as i32) * n, vh, ch);
                    wake_statusline();
                } else if is_key(Code::KeyF, "f") {
                    // Handled above when Ctrl+F focuses search.
                } else if is_key(Code::KeyB, "b") {
                    e.stop_propagation();
                    e.prevent_default();
                    pending_g.set_if_modified(false);
                    let n = take_count(&mut count_buf);
                    scroll_by(&mut scroll_controller, -((vh * 0.9) as i32) * n, vh, ch);
                    wake_statusline();
                }
                return;
            }

            if !plain {
                return;
            }

            let vh = (*view_height.peek()).max(240.0);
            let ch = (*content_height.peek()).max(vh);

            if *pending_g.peek() {
                pending_g.set(false);
                if let Key::Character(k) = &e.key {
                    e.stop_propagation();
                    e.prevent_default();
                    let n = take_count(&mut count_buf);
                    match k.as_str() {
                        "g" => {
                            let _ = scroll_controller.scroll_to_y(0);
                        }
                        "t" => step_group(n as isize, &mut active_group, &mut search_query),
                        "T" => step_group(-(n as isize), &mut active_group, &mut search_query),
                        _ => {}
                    }
                    wake_statusline();
                }
                return;
            }

            match &e.key {
                Key::Character(d) if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => {
                    let digit = d.as_bytes()[0] as char;
                    if digit != '0' || !count_buf.peek().is_empty() {
                        e.stop_propagation();
                        e.prevent_default();
                        let mut cur = count_buf.peek().clone();
                        if cur.len() < 3 {
                            cur.push(digit);
                            count_buf.set(cur);
                        }
                        wake_statusline();
                    }
                }
                Key::Character(k) if k == "j" => {
                    e.stop_propagation();
                    e.prevent_default();
                    let n = take_count(&mut count_buf);
                    scroll_by(&mut scroll_controller, 60 * n, vh, ch);
                    wake_statusline();
                }
                Key::Named(NamedKey::ArrowDown) => {
                    e.stop_propagation();
                    e.prevent_default();
                    let n = take_count(&mut count_buf);
                    scroll_by(&mut scroll_controller, 60 * n, vh, ch);
                }
                Key::Character(k) if k == "k" => {
                    e.stop_propagation();
                    e.prevent_default();
                    let n = take_count(&mut count_buf);
                    scroll_by(&mut scroll_controller, -60 * n, vh, ch);
                    wake_statusline();
                }
                Key::Named(NamedKey::ArrowUp) => {
                    e.stop_propagation();
                    e.prevent_default();
                    let n = take_count(&mut count_buf);
                    scroll_by(&mut scroll_controller, -60 * n, vh, ch);
                }
                Key::Character(k) if k == "G" => {
                    e.stop_propagation();
                    e.prevent_default();
                    let _ = take_count(&mut count_buf);
                    let max_y = (ch - vh).max(0.0).round() as i32;
                    scroll_controller.scroll_to_y(-max_y);
                    wake_statusline();
                }
                Key::Character(k) if k == "g" => {
                    e.stop_propagation();
                    e.prevent_default();
                    pending_g.set(true);
                    wake_statusline();
                }
                Key::Character(k) if k == "J" => {
                    e.stop_propagation();
                    e.prevent_default();
                    let n = take_count(&mut count_buf);
                    step_group(n as isize, &mut active_group, &mut search_query);
                    wake_statusline();
                }
                Key::Character(k) if k == "K" => {
                    e.stop_propagation();
                    e.prevent_default();
                    let n = take_count(&mut count_buf);
                    step_group(-(n as isize), &mut active_group, &mut search_query);
                    wake_statusline();
                }
                Key::Character(k) if k == "/" || k == "i" => {
                    e.stop_propagation();
                    e.prevent_default();
                    count_buf.set_if_modified(String::new());
                    let next = search_focus_seq.peek().wrapping_add(1);
                    search_focus_seq.set(next);
                    search_a11y_id.request_focus();
                    wake_statusline();
                }
                Key::Character(k) if k == "s" => {
                    e.stop_propagation();
                    e.prevent_default();
                    count_buf.set_if_modified(String::new());
                    let mut ja = jump_active;
                    ja.set(true);
                    wake_statusline();
                }
                Key::Character(k) if k == "q" => {
                    e.stop_propagation();
                    e.prevent_default();
                    count_buf.set_if_modified(String::new());
                    if closable {
                        Platform::get().close_window(Platform::window_id());
                    }
                }
                _ => {}
            }
        };

        let content_el: Element = if is_searching {
            SettingsSearchResults {
                query: current_query.clone(),
                on_clear: EventHandler::new(move |_| search_query.set(String::new())),
                on_jump_group: EventHandler::new(move |group| {
                    search_query.set(String::new());
                    active_group.set(group);
                }),
            }
            .into_element()
        } else {
            match current_group {
                SettingsGroupId::LookAndFeel => LookAndFeelGroup.into_element(),
                SettingsGroupId::TabManagement => TabManagementGroup.into_element(),
                SettingsGroupId::Spaces => SpacesGroup.into_element(),
                SettingsGroupId::Glance => GlanceGroup.into_element(),
                SettingsGroupId::Vim => VimSettingsGroup.into_element(),
                SettingsGroupId::Agents => AgentsSettingsGroup.into_element(),
                SettingsGroupId::WebChat => WebChatSettingsGroup.into_element(),
                SettingsGroupId::Plugins => PluginsGroup.into_element(),
                SettingsGroupId::Extensions => ExtensionsAndServicesGroup.into_element(),
            }
        };

        let show_search_jump_badge = *jump_active.read();
        let search_bar = rect()
            .width(Size::fill())
            .height(Size::px(38.0))
            .padding(Gaps::new(0.0, 10.0, 0.0, 12.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(palette.input_bg)
            .border(
                Border::new()
                    .width(1.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(if search_focused {
                        theme.color(ThemeRole::Accent)
                    } else {
                        palette.input_border
                    }),
            )
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .child(Icon::new("search-glass").size(15.0).role(ThemeRole::TextMuted))
            .child(
                rect().width(Size::flex(1.0)).font_size(13.5).child(
                    TextField::new(search_query)
                        .a11y_id(search_a11y_id)
                        .select_all_on_open(true)
                        .select_all_seq(*search_focus_seq.read())
                        .placeholder("Find in Settings")
                        .width(Size::fill())
                        .padding(Gaps::new(0.0, 2.0, 0.0, 2.0))
                        .background(Color::TRANSPARENT)
                        .focus_background(Color::TRANSPARENT)
                        .color(theme.color(ThemeRole::Text))
                        .placeholder_color(theme.color(ThemeRole::TextMuted)),
                ),
            )
            .maybe_child(show_search_jump_badge.then(|| {
                rect()
                    .height(Size::px(16.0))
                    .padding(Gaps::new(0.0, 5.0, 0.0, 5.0))
                    .corner_radius(CornerRadius::new_all(4.0))
                    .background(Color::from_rgb(0xa2, 0x57, 0x72))
                    .center()
                    .child(
                        label()
                            .text("a")
                            .font_size(11.0)
                            .font_weight(FontWeight::BOLD)
                            .font_family("monospace")
                            .color(Color::from_rgb(0xee, 0xf5, 0xff)),
                    )
            }))
            .maybe_child(is_searching.then(|| {
                IconButton::new("close", "Clear search")
                    .size(22.0)
                    .icon_size(12.0)
                    .on_press(move |_| search_query.set(String::new()))
            }));

        let h_pad = if compact { 20.0 } else { 44.0 };

        // Settings-local Vim statusline overlay (`PG-01`).
        let settings_mode = if any_field_focused {
            InputMode::Insert
        } else {
            InputMode::Normal
        };
        let mut typed_str = count_buf.read().clone();
        if *pending_g.read() {
            typed_str.push('g');
        }
        let show_sl = vim_enabled
            && statusline_settings.enabled
            && (statusline_settings.show != StatuslineShow::Never || !typed_str.is_empty() || *sl_awake.read());
        let sl_overlay = show_sl.then(|| {
            let resting =
                statusline_settings.show == StatuslineShow::Shrink && typed_str.is_empty() && !*sl_awake.read();
            let sz = if resting {
                (MASCOT_FULL_SIZE * MASCOT_REST_SCALE).round()
            } else {
                MASCOT_FULL_SIZE
            };
            rect()
                .layer(Layer::OverlayLevel(CHROME_OVERLAY_LEVEL))
                .position(
                    Position::new_absolute()
                        .right(STATUSLINE_CORNER_GAP)
                        .bottom(STATUSLINE_CORNER_GAP),
                )
                .direction(Direction::Horizontal)
                .cross_align(Alignment::End)
                .spacing(6.0)
                .interactive(Interactive::No)
                .maybe_child((!typed_str.is_empty()).then(|| {
                    rect()
                        .padding(Gaps::new(1.0, 8.0, 1.0, 8.0))
                        .margin(Gaps::new(0.0, 0.0, 2.0, 0.0))
                        .corner_radius(CornerRadius::new_all(8.0))
                        .background(theme.color(ThemeRole::StatuslineBackground))
                        .border(
                            Border::new()
                                .width(2.0)
                                .alignment(BorderAlignment::Inner)
                                .fill(theme.palette.ink.to_color()),
                        )
                        .child(
                            label()
                                .text(typed_str)
                                .font_size(12.0)
                                .font_weight(FontWeight::BOLD)
                                .font_family("monospace")
                                .color(theme.color(ThemeRole::StatuslineText)),
                        )
                }))
                .child(VimMascot::new(settings_mode).size(sz))
        });

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .background(palette.canvas_bg)
            .color(theme.color(ThemeRole::Text))
            .on_sized(move |e: Event<SizedEventData>| {
                let w = e.area.width();
                let h = e.area.height();
                if w > 0.0 && (*view_width.peek() - w).abs() > 1.0 {
                    view_width.set(w);
                }
                if h > 0.0 && (*view_height.peek() - h).abs() > 1.0 {
                    view_height.set(h);
                }
            })
            .on_global_key_down(on_global_key_down)
            .child(SettingsSidebar {
                active_group: current_group,
                searching: is_searching,
                compact,
                on_select_group: EventHandler::new(move |group| {
                    search_query.set(String::new());
                    active_group.set(group);
                }),
            })
            .child(
                ScrollView::new_controlled(scroll_controller)
                    .width(Size::flex(1.0))
                    .height(Size::fill())
                    .child(
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new(24.0, h_pad, 40.0, h_pad))
                            .cross_align(Alignment::center())
                            .on_sized(move |e: Event<SizedEventData>| {
                                let h = e.area.height();
                                if h > 0.0 && (*content_height.peek() - h).abs() > 1.0 {
                                    content_height.set(h);
                                }
                            })
                            .child(
                                rect()
                                    .width(Size::fill())
                                    .max_width(Size::px(760.0))
                                    .spacing(22.0)
                                    .child(search_bar)
                                    .child(content_el),
                            ),
                    ),
            )
            .maybe_child(sl_overlay)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
