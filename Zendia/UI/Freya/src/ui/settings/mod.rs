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

use crate::ui::core_view::use_provide_core_with_observer;
use crate::ui::kit::{Icon, IconButton, TextField, use_provide_text_focus};
use crate::ui::settings::controls::SettingsPalette;
use crate::ui::settings::look_and_feel::LookAndFeelGroup;
use crate::ui::settings::plugins_and_extensions::{ExtensionsAndServicesGroup, PluginsGroup};
use crate::ui::settings::search_results::SettingsSearchResults;
use crate::ui::settings::sidebar::SettingsSidebar;
use crate::ui::settings::tabs_and_spaces::{GlanceGroup, SpacesGroup, TabManagementGroup};
use crate::ui::settings::zendia_groups::{AgentsSettingsGroup, VimSettingsGroup, WebChatSettingsGroup};
use crate::ui::theme::{ThemeRole, use_provide_theme, use_theme};
use freya::prelude::*;
use std::sync::{Arc, Mutex};
use zendia_browser_core::BrowserCore;
use zendia_browser_core::settings::SettingsGroupId;

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
        let theme = use_theme();
        let palette = SettingsPalette::for_scheme(theme.scheme);
        let search_a11y_id = use_hook(AccessibilityId::new_unique);
        let search_focus = use_focus(search_a11y_id);
        let mut search_focus_seq = use_state(|| 0u64);
        let mut view_width = use_state(|| 1080.0_f32);
        let mut scroll_controller = use_scroll_controller(ScrollConfig::default);
        let mut active_group = self.active_group;
        let mut search_query = self.search_query;

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

        let on_global_key_down = move |e: Event<KeyboardEventData>| {
            if e.modifiers.intersects(Modifiers::CONTROL | Modifiers::META) && e.code == Code::KeyF {
                e.stop_propagation();
                e.prevent_default();
                let next = search_focus_seq.peek().wrapping_add(1);
                search_focus_seq.set(next);
                search_a11y_id.request_focus();
                return;
            }
            if matches!(e.key, Key::Named(NamedKey::Escape)) {
                if !search_query.peek().is_empty() {
                    search_query.set(String::new());
                } else if closable {
                    Platform::get().close_window(Platform::window_id());
                }
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

        let search_focused = search_focus() != Focus::Not;
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
            .maybe_child(is_searching.then(|| {
                IconButton::new("close", "Clear search")
                    .size(22.0)
                    .icon_size(12.0)
                    .on_press(move |_| search_query.set(String::new()))
            }));

        let h_pad = if compact { 20.0 } else { 44.0 };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .background(palette.canvas_bg)
            .color(theme.color(ThemeRole::Text))
            .on_sized(move |e: Event<SizedEventData>| {
                let w = e.area.width();
                if w > 0.0 && (*view_width.peek() - w).abs() > 1.0 {
                    view_width.set(w);
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
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
