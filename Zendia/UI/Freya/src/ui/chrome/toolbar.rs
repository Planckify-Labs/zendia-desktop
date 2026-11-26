/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Toolbar surfaces: contributed toolbar buttons, the address bar slot, window controls, and the top toolbar.

use crate::ui::chrome::address_bar::try_use_address_bar_state;
use crate::ui::chrome::layout::{
    COMPACT_TOP_BAR_HEIGHT, ChromePalette, ELEMENT_SEPARATION, TOP_TOOLBAR_HEIGHT, ToolbarSlots, URL_BAR_HEIGHT_SINGLE,
    URL_BAR_HEIGHT_TOP, show_window_controls_in_top_bar,
};
use crate::ui::chrome::menus::{AnchorRect, open_extension_context_menu, try_use_menu_state};
use crate::ui::chrome::sidebar::state::try_use_sidebar_state;
use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, KitTooltip, LibrarySprite, use_interactive_feedback};
use crate::ui::page::download_arc::try_use_download_arc;
use crate::ui::panels::{active_agent_accent, try_use_dock_state};
use crate::ui::theme::{ResolvedScheme, ThemeRole, motion, use_theme};
use crate::ui::vim::explorer::is_explorer_active;
use crate::ui::vim::hints::hint_badge_for_corner;
use crate::ui::vim::toolbar_focus::{apply_toolbar_cursor_bg, apply_toolbar_cursor_border, is_toolbar_cursor};
use crate::ui::webchat::logos::parse_hex_color;
use crate::ui::webchat::views::try_use_webchat_views;
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::address_bar::display_url_for_sidebar;
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::{BadgeTone, ContributionSource, ToolbarBadge, ToolbarItem, ToolbarItemKind};
use zendia_browser_core::focus::FocusTarget;
use zendia_browser_core::model::{ConversationStatus, ServiceId, ServiceStatus, SidebarSide};
use zendia_browser_core::modules::service_status_in;
use zendia_keymap::KeyTable;

const ICON_MINIMIZE_SVG: &[u8] =
    br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><path d="M2.5 6.5h7" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/></svg>"#;
const ICON_MAXIMIZE_SVG: &[u8] =
    br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><rect x="2.5" y="2.5" width="7" height="7" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.3"/></svg>"#;
const ICON_CLOSE_SVG: &[u8] =
    br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/></svg>"#;

/// Renders one contributed [`ToolbarItem`] from the core's toolbar model.
#[derive(Clone, PartialEq)]
pub struct ToolbarItemView {
    pub item: ToolbarItem,
    pub location: State<String>,
    pub location_a11y_id: AccessibilityId,
    pub compact_urlbar: bool,
    pub dense: bool,
    pub tight: bool,
    /// Collapses the `"Chat"` pill text label when the top bar is narrower than `720 px` (`TB-01`).
    pub hide_chat_label: bool,
    key: DiffKey,
}

impl KeyExt for ToolbarItemView {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ToolbarItemView {
    pub fn new(item: ToolbarItem, location: State<String>, location_a11y_id: AccessibilityId) -> Self {
        Self {
            item,
            location,
            location_a11y_id,
            compact_urlbar: false,
            dense: false,
            tight: false,
            hide_chat_label: false,
            key: DiffKey::None,
        }
    }

    pub fn compact_urlbar(mut self, compact: bool) -> Self {
        self.compact_urlbar = compact;
        self
    }

    pub fn dense(mut self, dense: bool) -> Self {
        self.dense = dense;
        self
    }

    pub fn tight(mut self, tight: bool) -> Self {
        self.tight = tight;
        self
    }

    pub fn hide_chat_label(mut self, hide: bool) -> Self {
        self.hide_chat_label = hide;
        self
    }
}

/// The library button, where downloads land.
const LIBRARY_ITEM_ID: &str = "builtin.library";
/// Side of the library button's animated icon.
const EXTENSIONS_ITEM_ID: &str = "builtin.extensions";
const CREATE_NEW_ITEM_ID: &str = "builtin.new_tab";

/// The icon of `item`, mirrored where the fork mirrors it for a sidebar on the right.
fn sided_icon(item: &ToolbarItem, sidebar_side: SidebarSide) -> String {
    match item.command.as_str() {
        "layout.toggle_compact_mode" if sidebar_side == SidebarSide::Right => "sidebar-right".to_string(),
        _ => item.icon.clone(),
    }
}

/// One row of the sidebar top row's `»` menu: an overflowed button's icon and name, as the fork lists them.
#[derive(Clone, PartialEq)]
pub struct OverflowMenuRow {
    pub item: ToolbarItem,
    /// Whether the menu holding this row is open.
    pub menu_open: State<bool>,
}

impl Component for OverflowMenuRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let sidebar_state = try_use_sidebar_state();
        let feedback = use_interactive_feedback(true);
        let sidebar_side = core.model().read().layout.sidebar_side;
        let icon = sided_icon(&self.item, sidebar_side);
        let item = self.item.clone();
        let mut menu_open = self.menu_open;

        rect()
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.item.title.clone())
            .width(Size::fill())
            .height(Size::px(34.0))
            .padding(Gaps::new(0.0, 16.0, 0.0, 16.0))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(12.0)
            .background(feedback.background(Color::TRANSPARENT, palette.surface_pill, palette.surface_pill_hover))
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .on_pointer_down(feedback.on_pointer_down())
            .on_global_pointer_up(feedback.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                menu_open.set(false);
                // The fork leaves the sidebar out under the pointer after Compact Mode is picked from this menu.
                if item.command == "layout.toggle_compact_mode"
                    && !core.model().peek().layout.compact_mode
                    && let Some(state) = sidebar_state
                {
                    let mut revealed = state.compact_revealed;
                    revealed.set_if_modified(true);
                }
                match Command::from_name_and_args(&item.command, &item.command_args) {
                    Ok(command) => {
                        core.invoke_or_log(command);
                    }
                    Err(error) => eprintln!("zendia: invalid toolbar command `{}`: {error}", item.command),
                }
            })
            .child(Icon::new(icon).size(16.0).role(ThemeRole::Text))
            .child(
                label()
                    .text(self.item.title.clone())
                    .font_size(13.5)
                    .color(palette.text),
            )
    }
}

/// Partitions single-toolbar sidebar top-row buttons into `(visible_start, visible_end, overflow)` so `Single + Right`
/// never crams 8 buttons into a narrow sidebar (`TB-02`).
pub fn partition_sidebar_top_row(
    start: &[ToolbarItem],
    end: &[ToolbarItem],
    has_window_controls: bool,
    sidebar_width: f32,
) -> (Vec<ToolbarItem>, Vec<ToolbarItem>, Vec<ToolbarItem>) {
    let inner_w = (sidebar_width - 14.0).max(80.0);
    let btn_step = 28.0;
    let controls_w = if has_window_controls {
        OVERFLOW_WINDOW_CONTROLS_RESERVE
    } else {
        0.0
    };
    let total_buttons = start.len() + end.len();
    if (total_buttons as f32 * btn_step + controls_w) <= inner_w {
        return (start.to_vec(), end.to_vec(), Vec::new());
    }

    // Reserve space for the `>>` overflow button (`24px`) plus window controls; keep app menu + back/forward visible
    // first and spill extra buttons (`sidebar_toggle`, `reload`, etc.) into the overflow popover like the fork.
    let budget = (inner_w - controls_w - 24.0).max(btn_step);
    let max_visible = (budget / btn_step).floor().max(1.0) as usize;

    let mut vis_start = Vec::new();
    let mut vis_end = Vec::new();
    let mut overflow = Vec::new();

    // Always keep the first start button (App Menu `...`) if present.
    if let Some(first) = start.first() {
        vis_start.push(first.clone());
    }
    let mut remaining = max_visible.saturating_sub(vis_start.len());

    // Prefer keeping back/forward navigation before secondary buttons.
    for item in end {
        if remaining > 0 {
            vis_end.push(item.clone());
            remaining -= 1;
        } else {
            overflow.push(item.clone());
        }
    }
    for item in start.iter().skip(1) {
        if remaining > 0 {
            vis_start.push(item.clone());
            remaining -= 1;
        } else {
            overflow.push(item.clone());
        }
    }

    (vis_start, vis_end, overflow)
}

impl Component for ToolbarItemView {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let menu_state = try_use_menu_state();
        let download_arc = try_use_download_arc();
        let dock_state = try_use_dock_state();
        let webchat_views = try_use_webchat_views();
        let key_table = use_hook(KeyTable::standard);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let landing_seq = if self.item.id.as_str() == LIBRARY_ITEM_ID {
            download_arc.map(|arc| *arc.landing_seq.read()).unwrap_or(0)
        } else {
            0
        };
        let landing_anim = use_animation_with_dependencies(&landing_seq, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(Duration::from_millis(280))
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let landing_t = landing_anim.get().value().clamp(0.0, 1.0);
        let landing_scale = if landing_seq > 0 {
            1.0 + 0.22 * (4.0 * landing_t * (1.0 - landing_t))
        } else {
            1.0
        };

        let is_create_new = self.item.id.as_str() == CREATE_NEW_ITEM_ID;
        let create_new_open = is_create_new && menu_state.is_some_and(|state| *state.create_new_open.read());
        let create_rotate = use_animation_with_dependencies(&create_new_open, move |conf, open| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *open { (0.0, 45.0) } else { (45.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::CREATE_NEW_ROTATE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let create_angle = if is_create_new {
            create_rotate.get().value()
        } else {
            0.0
        };

        let is_chat_button = self.item.id.as_str() == "zendia.chat";
        let is_agents_button = self.item.id.as_str() == "zendia.agents";
        let is_webchat_button = self.item.id.as_str() == "zendia.webchat";
        let agent_open = dock_state.is_some_and(|ds| ds.is_agent_open());
        let agents_open = dock_state.is_some_and(|ds| ds.is_agents_open());
        let webchat_open = dock_state.is_some_and(|ds| ds.is_webchat_open());
        let static_snapshot = dock_state.is_some_and(|ds| ds.pinned_ring_phase.read().is_some());

        let (
            active_tab_id,
            can_back,
            can_forward,
            is_loading,
            sidebar_side,
            chat_status_dot,
            webchat_status_dot,
            dynamic_tooltip,
            extension_paused,
        ) = {
            let model = core.model().read();
            let active = model.active_tab();
            let is_extension_item = matches!(self.item.source, ContributionSource::Extension { .. });
            let ext_paused = is_extension_item
                && !matches!(
                    service_status_in(&model.services, ServiceId::ExtensionServer),
                    ServiceStatus::Running
                );

            // TB-01: Scope Chat status dot strictly to the active tab's conversation while closed.
            let (chat_dot, chat_tip) = if is_chat_button {
                let server_running = model
                    .services
                    .iter()
                    .any(|s| s.service == ServiceId::AgentServer && matches!(s.status, ServiceStatus::Running));
                let tab_conv = active.and_then(|t| model.active_conversation_for_tab(t.id));
                let running = tab_conv.is_some_and(|c| {
                    matches!(
                        c.status,
                        ConversationStatus::Working | ConversationStatus::WaitingForUser
                    )
                });
                let unseen = tab_conv.is_some_and(|c| c.unseen);
                let agent_name = tab_conv
                    .and_then(|c| model.agent(&c.agent_id).map(|a| a.name.clone()))
                    .or_else(|| model.default_agent().map(|a| a.name.clone()))
                    .unwrap_or_else(|| "Your agent".to_string());
                let dot = if agent_open || !server_running || (!running && !unseen) {
                    None
                } else {
                    Some((active_agent_accent(&model, &theme), running))
                };
                let tip = if agent_open {
                    "Close chat".to_string()
                } else if running {
                    format!("{agent_name} is working. Open chat")
                } else if unseen {
                    format!("{agent_name} finished. Open chat")
                } else {
                    "Chat with your agent about this tab".to_string()
                };
                (dot, Some(tip))
            } else {
                (None, None)
            };

            // TB-02: Dynamic Agents tooltip.
            let agents_tip =
                is_agents_button.then(|| zendia_browser_core::agent::agents_button_tooltip(&model, agents_open));

            // TB-03: Site-colored Web chat status dot & dynamic tooltip while closed.
            let (wc_dot, wc_tip) = if is_webchat_button {
                let site = model.active_chat_site();
                let site_color = site
                    .map(|s| parse_hex_color(&s.accent_hex, theme.color(ThemeRole::Accent)))
                    .unwrap_or_else(|| theme.color(ThemeRole::Accent));
                let wc_loading = site.is_some_and(|s| {
                    webchat_views.is_some_and(|reg| reg.view_for_site(&s.id).is_some() && reg.is_loading(&s.id))
                });
                let wc_pending = model.webchat_pending_prompt.is_some();
                let dot = if webchat_open || (!wc_loading && !wc_pending) {
                    None
                } else {
                    Some((site_color, wc_loading || wc_pending))
                };
                let tip = if webchat_open {
                    "Close web chat".to_string()
                } else if let Some(s) = site {
                    format!("Chat with {} on the web", s.name)
                } else {
                    "Chat with ChatGPT, Claude, Gemini and more on the web".to_string()
                };
                (dot, Some(tip))
            } else {
                (None, None)
            };

            (
                active.map(|tab| tab.id),
                active.is_some_and(|tab| tab.can_navigate_back),
                active.is_some_and(|tab| tab.can_navigate_forward),
                active.is_some_and(|tab| tab.is_loading),
                model.layout.sidebar_side,
                chat_dot,
                wc_dot,
                chat_tip.or(agents_tip).or(wc_tip),
                ext_paused,
            )
        };

        let effective_enabled = if self.item.kind == ToolbarItemKind::UrlBar {
            false
        } else if extension_paused {
            true
        } else if !self.item.enabled {
            false
        } else {
            match self.item.command.as_str() {
                "navigation.back" => can_back,
                "navigation.forward" => can_forward,
                "navigation.reload" => active_tab_id.is_some(),
                _ => true,
            }
        };

        let feedback = use_interactive_feedback(effective_enabled);

        if self.item.kind == ToolbarItemKind::UrlBar {
            return LocationBar {
                location: self.location,
                location_a11y_id: self.location_a11y_id,
                compact_sidebar: self.compact_urlbar,
            }
            .into_element();
        }

        let effective_icon = match self.item.command.as_str() {
            "navigation.reload" if is_loading => "close".to_string(),
            _ => sided_icon(&self.item, sidebar_side),
        };

        let is_app_menu = self.item.kind == ToolbarItemKind::AppMenu;
        let is_extensions_menu = self.item.id.as_str() == EXTENSIONS_ITEM_ID;
        let popup_open = (is_app_menu && menu_state.is_some_and(|state| *state.app_menu_open.read()))
            || (is_extensions_menu && menu_state.is_some_and(|state| *state.extensions_menu_open.read()))
            || create_new_open;
        let is_pressed = popup_open
            || (is_chat_button && agent_open)
            || (is_agents_button && agents_open)
            || (is_webchat_button && webchat_open);
        let is_zendia_capsule = self.item.id.as_str().starts_with("zendia.");

        let item = self.item.clone();
        let press_core = core.clone();
        let on_press = move |_: Event<PressEventData>| {
            if !effective_enabled {
                return;
            }
            // TB-03: Bottom `+` opens the Create New popup (New Space / Folder / Split / Tab) with 45° rotation.
            if is_create_new && let Some(state) = menu_state {
                state.toggle_create_new(None);
                return;
            }
            if item.command == "navigation.reload" && is_loading {
                press_core.invoke_or_log(Command::NavigationStop { tab_id: active_tab_id });
                return;
            }
            match Command::from_name_and_args(&item.command, &item.command_args) {
                Ok(command) => {
                    press_core.invoke_or_log(command);
                }
                Err(error) => {
                    eprintln!("zendia: invalid toolbar command `{}`: {error}", item.command);
                }
            }
        };

        let footer_target = FocusTarget::SidebarFooter(self.item.id.clone());
        let (focus_border, explorer_footer_focused, footer_hint_badge) = {
            let base = if focus() == Focus::Keyboard {
                Border::new()
                    .width(1.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::FocusRing))
            } else {
                Border::new().width(0.0).fill(Color::TRANSPARENT)
            };
            let model = core.model().read();
            let tb_border = apply_toolbar_cursor_border(base, &model, self.item.id.as_str(), &theme);
            let is_ex_foot = is_explorer_active(&model) && model.focus.is_cursor(&footer_target);
            let final_border = if is_ex_foot {
                Border::new()
                    .width(2.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::ModeExplorer))
            } else {
                tb_border
            };
            let hint = hint_badge_for_corner(&model, &footer_target, &theme);
            (final_border, is_ex_foot, hint)
        };

        let a11y_label = dynamic_tooltip
            .or_else(|| self.item.tooltip.clone())
            .unwrap_or_else(|| self.item.title.clone());
        let tooltip_shortcut = key_table
            .shortcut_for_command(&self.item.command)
            .map(|seq| seq.display_human());
        // Side-panel toggle buttons keep their dynamic "Close ..." tooltip while pressed, matching the fork;
        // popup-menu buttons hide their tooltip while the popup is open.
        let tooltip_enabled = effective_enabled && !popup_open;
        let active_press_fill = theme.rgba(ThemeRole::Text).with_alpha(0.26).to_color();

        if self.item.kind == ToolbarItemKind::PillButton {
            let (rest_bg, hover_bg) = if explorer_footer_focused {
                let focus_fill = theme.rgba(ThemeRole::ModeExplorer).with_alpha(0.30).to_color();
                (focus_fill, focus_fill)
            } else if is_pressed {
                (active_press_fill, active_press_fill)
            } else {
                (palette.surface_pill, palette.surface_pill_hover)
            };
            let base_bg = feedback.background(rest_bg, hover_bg, active_press_fill);
            let bg = apply_toolbar_cursor_bg(base_bg, &core.model().read(), self.item.id.as_str(), &theme);
            let pill_h = if self.tight { 24.0 } else { 26.0 };
            let hide_label = is_chat_button && self.hide_chat_label;
            let pill_pad = if hide_label {
                Gaps::new(0.0, 6.0, 0.0, 6.0)
            } else if self.tight {
                Gaps::new(0.0, 7.0, 0.0, 6.0)
            } else {
                Gaps::new(0.0, 11.0, 0.0, 8.0)
            };
            let pill = rect()
                .a11y_id(a11y_id)
                .a11y_focusable(effective_enabled)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(a11y_label.clone())
                .height(Size::px(pill_h))
                .padding(pill_pad)
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .main_align(Alignment::center())
                .spacing(if hide_label {
                    0.0
                } else if self.tight {
                    3.5
                } else {
                    6.0
                })
                .corner_radius(CornerRadius::new_all(999.0))
                .background(bg)
                .border(focus_border)
                .scale(feedback.scale())
                .opacity(if effective_enabled { 1.0 } else { 0.4 })
                .cursor(if effective_enabled {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                })
                .on_pointer_enter(feedback.on_pointer_enter())
                .on_pointer_leave(feedback.on_pointer_leave())
                .on_pointer_down(feedback.on_pointer_down())
                .on_global_pointer_up(feedback.on_pointer_up())
                .on_press(on_press)
                .child(
                    Icon::new(effective_icon)
                        .size(if self.tight { 12.5 } else { 14.0 })
                        .role(ThemeRole::Text),
                )
                .maybe_child((!hide_label).then(|| {
                    label()
                        .text(self.item.title.clone())
                        .font_size(if self.tight { 11.0 } else { 12.0 })
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(palette.text)
                        .max_lines(1)
                }))
                .maybe_child(footer_hint_badge);
            let pill_with_dot: Element = if let Some((dot_col, pulsing)) = chat_status_dot {
                // Anchor the 8px status dot at `top: -1px, right: -1px` of the pill via a 0-width trailing box so
                // Torin resolves its offset from the pill's own right edge rather than the toolbar row's width.
                rect()
                    .direction(Direction::Horizontal)
                    .height(Size::px(pill_h))
                    .cross_align(Alignment::center())
                    .child(pill)
                    .child(
                        rect()
                            .width(Size::px(0.0))
                            .height(Size::px(pill_h))
                            .interactive(Interactive::No)
                            .child(ToolbarPulseDot {
                                color: dot_col,
                                pulsing: pulsing && !static_snapshot,
                                left_offset: Some(-7.0),
                                key: DiffKey::from(&"chat-status-dot"),
                            }),
                    )
                    .into_element()
            } else {
                pill.into_element()
            };
            return KitTooltip::new(a11y_label)
                .maybe_shortcut(tooltip_shortcut)
                .enabled(tooltip_enabled)
                .child(pill_with_dot)
                .into_element();
        }

        let button_width = if self.tight {
            20.0
        } else if self.dense {
            26.0
        } else {
            28.0
        };
        let button_height = if self.tight {
            24.0
        } else if self.dense {
            26.0
        } else {
            28.0
        };
        let icon_size = if is_zendia_capsule {
            if self.tight { 12.5 } else { 14.0 }
        } else if self.tight {
            12.5
        } else {
            15.0
        };
        let radius = if is_zendia_capsule { 999.0 } else { 6.0 };

        let bg = {
            let (rest, hover) = if explorer_footer_focused {
                let focus_fill = theme.rgba(ThemeRole::ModeExplorer).with_alpha(0.30).to_color();
                (focus_fill, focus_fill)
            } else if is_pressed {
                (active_press_fill, active_press_fill)
            } else if is_zendia_capsule {
                (palette.surface_pill, palette.surface_pill_hover)
            } else {
                (Color::TRANSPARENT, palette.surface_pill)
            };
            let base = feedback.background(rest, hover, active_press_fill);
            apply_toolbar_cursor_bg(base, &core.model().read(), self.item.id.as_str(), &theme)
        };

        let icon_role = if extension_paused {
            ThemeRole::Warning
        } else if effective_enabled {
            if is_zendia_capsule || feedback.is_hovered() || is_pressed {
                ThemeRole::Text
            } else {
                ThemeRole::TextMuted
            }
        } else {
            ThemeRole::TextMuted
        };

        let effective_badge = if is_agents_button {
            zendia_browser_core::agent::agents_button_badge_for_state(&core.model().read(), agents_open)
        } else if extension_paused {
            Some(ToolbarBadge {
                text: "II".to_string(),
                tone: BadgeTone::Warning,
            })
        } else {
            self.item.badge.clone()
        };

        let ext_info = match &self.item.source {
            ContributionSource::Extension { extension_id } => core
                .model()
                .read()
                .extension(&zendia_browser_core::modules::ExtensionId::new(extension_id.clone()))
                .cloned(),
            _ => None,
        };
        let is_extension_item = ext_info.is_some();

        let badge: Option<Element> = effective_badge.map(|badge| {
            let (badge_bg, badge_fg) = if !extension_paused
                && let Some(ext) = &ext_info
                && let Some([r, g, b, a]) = ext.badge_bg
            {
                let bg = Color::from_argb(a, r, g, b);
                let fg = ext
                    .badge_fg
                    .map(|[fr, fg, fb, fa]| Color::from_argb(fa, fr, fg, fb))
                    .unwrap_or(Color::WHITE);
                (bg, fg)
            } else {
                match badge.tone {
                    BadgeTone::Neutral => (theme.color(ThemeRole::SurfaceRaised), theme.color(ThemeRole::Text)),
                    BadgeTone::Accent => (theme.color(ThemeRole::Accent), theme.color(ThemeRole::TextOnAccent)),
                    BadgeTone::Warning => (theme.color(ThemeRole::Warning), theme.color(ThemeRole::Ink)),
                    BadgeTone::Error => (theme.color(ThemeRole::Danger), Color::WHITE),
                }
            };
            if is_extension_item {
                // Compact Firefox/Zen `.toolbarbutton-badge` tucked cleanly inside the top-right corner of the button.
                rect()
                    .position(Position::new_absolute().top(1.0).right(1.0))
                    .min_width(Size::px(14.0))
                    .height(Size::px(13.0))
                    .padding(Gaps::new(0.0, 3.0, 0.0, 3.0))
                    .corner_radius(CornerRadius::new_all(3.0))
                    .background(badge_bg)
                    .center()
                    .interactive(Interactive::No)
                    .child(
                        label()
                            .text(badge.text)
                            .font_size(8.5)
                            .font_weight(FontWeight::BOLD)
                            .color(badge_fg),
                    )
                    .into_element()
            } else {
                ToolbarPulseBadge {
                    text: badge.text,
                    bg: badge_bg,
                    fg: badge_fg,
                    pulsing: is_agents_button && badge.tone == BadgeTone::Warning && !static_snapshot,
                    key: DiffKey::from(&format!("tb-badge-{}", self.item.id.as_str())),
                }
                .into_element()
            }
        });

        let icon_status_dot = webchat_status_dot.map(|(dot_col, pulsing)| ToolbarPulseDot {
            color: dot_col,
            pulsing: pulsing && !static_snapshot,
            left_offset: None,
            key: DiffKey::from(&"webchat-status-dot"),
        });

        let ext_ctx_id = match &self.item.source {
            ContributionSource::Extension { extension_id } => Some(extension_id.clone()),
            _ => None,
        };
        let ext_ctx_core = core.clone();
        let on_mouse_down_ext = move |e: Event<MouseEventData>| {
            if e.button == Some(MouseButton::Right)
                && let Some(ext_id) = &ext_ctx_id
                && let Some(ms) = &menu_state
            {
                e.stop_propagation();
                let model = ext_ctx_core.model().peek().clone();
                let pos = (e.global_location.x as f32, e.global_location.y as f32);
                open_extension_context_menu(ms, &model, ext_id, pos);
            }
        };

        let button = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(effective_enabled)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(a11y_label.clone())
            .width(Size::px(if is_zendia_capsule {
                if self.tight { 26.0 } else { 30.0 }
            } else {
                button_width
            }))
            .height(Size::px(if is_zendia_capsule {
                if self.tight { 24.0 } else { 26.0 }
            } else {
                button_height
            }))
            .corner_radius(CornerRadius::new_all(radius))
            .background(bg)
            .border(focus_border)
            .scale(feedback.scale() * landing_scale)
            .opacity(if extension_paused {
                0.58
            } else if effective_enabled {
                1.0
            } else {
                0.36
            })
            .center()
            .cursor(if effective_enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            })
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .on_pointer_down(feedback.on_pointer_down())
            .on_global_pointer_up(feedback.on_pointer_up())
            .on_mouse_down(on_mouse_down_ext)
            .on_press(on_press)
            .child(
                rect()
                    .rotate(create_angle)
                    .center()
                    .child(if self.item.id.as_str() == LIBRARY_ITEM_ID {
                        LibrarySprite::new(theme.color(icon_role), feedback.is_hovered()).into_element()
                    } else if let Some(ext) = &ext_info {
                        crate::ui::extensions::ExtensionIconView::new(
                            format!("tb-ext-{}", ext.id.as_str()),
                            effective_icon,
                            ext.icon_data.clone(),
                            if self.tight { 14.0 } else { 16.0 },
                            icon_role,
                        )
                        .into_element()
                    } else {
                        Icon::new(effective_icon).size(icon_size).role(icon_role).into_element()
                    }),
            )
            .maybe_child(badge)
            .maybe_child(icon_status_dot)
            .maybe_child(footer_hint_badge);

        let wrapped = if is_app_menu {
            button
                .on_sized(move |e: Event<SizedEventData>| {
                    if let Some(state) = menu_state {
                        let mut anchor = state.app_menu_anchor;
                        anchor.set_if_modified(AnchorRect::from_area(e.area));
                    }
                })
                .into_element()
        } else if is_extensions_menu {
            button
                .on_sized(move |e: Event<SizedEventData>| {
                    if let Some(state) = menu_state {
                        let mut anchor = state.extensions_menu_anchor;
                        anchor.set_if_modified(AnchorRect::from_area(e.area));
                    }
                })
                .into_element()
        } else if is_create_new {
            button
                .on_sized(move |e: Event<SizedEventData>| {
                    if let Some(state) = menu_state {
                        let mut anchor = state.create_new_anchor;
                        anchor.set_if_modified(AnchorRect::from_area(e.area));
                    }
                })
                .into_element()
        } else if self.item.id.as_str() == LIBRARY_ITEM_ID
            && let Some(arc) = download_arc
        {
            // Downloads fly into the library button, so it records where it sits.
            button
                .on_sized(move |e: Event<SizedEventData>| {
                    let mut anchor = arc.anchor;
                    let center = (e.area.center().x, e.area.center().y);
                    anchor.set_if_modified(Some(center));
                })
                .into_element()
        } else {
            button.into_element()
        };

        KitTooltip::new(a11y_label)
            .maybe_shortcut(tooltip_shortcut)
            .enabled(tooltip_enabled)
            .child(wrapped)
            .into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Address bar slot styled for either the compact single-toolbar sidebar or the centered top bar.
#[derive(Clone, PartialEq)]
pub struct LocationBar {
    pub location: State<String>,
    pub location_a11y_id: AccessibilityId,
    pub compact_sidebar: bool,
}

impl Component for LocationBar {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let addr_state = try_use_address_bar_state();
        let mut hovered = use_state(|| false);
        let mut copy_hovered = use_state(|| false);
        let mut site_btn_hovered = use_state(|| false);
        let focus = use_focus(self.location_a11y_id);
        let is_focused = focus() != Focus::Not;

        let (active_url, site_info, toolbar_cursor_on_urlbar) = {
            let model = core.model().read();
            let url = model
                .active_tab()
                .map(|tab| tab.url.clone())
                .unwrap_or_else(|| self.location.read().clone());
            let info = model.active_site_info();
            (url, info, is_toolbar_cursor(&model, "builtin.url_bar"))
        };

        let floating_open = addr_state.is_some_and(|s| *s.open.read());
        let site_panel_open = addr_state.is_some_and(|s| *s.site_panel_open.read());
        let is_web_page = site_info.security.is_web_page();

        let shown_text = if floating_open {
            String::new()
        } else if self.compact_sidebar {
            display_url_for_sidebar(&active_url)
        } else {
            active_url.clone()
        };
        let is_placeholder = shown_text.is_empty();
        let display_label = if floating_open {
            String::new()
        } else if is_placeholder {
            if self.compact_sidebar {
                "Search or enter URL".to_string()
            } else {
                "Search or enter an address".to_string()
            }
        } else {
            shown_text
        };

        let height = if self.compact_sidebar {
            URL_BAR_HEIGHT_SINGLE
        } else {
            URL_BAR_HEIGHT_TOP
        };
        let radius = if self.compact_sidebar {
            999.0
        } else if toolbar_cursor_on_urlbar {
            18.0
        } else {
            8.0
        };

        let bg = if is_focused {
            palette.urlbar_focus_bg
        } else if hovered() {
            palette.surface_pill_hover
        } else {
            palette.urlbar_bg
        };
        let border_color = if toolbar_cursor_on_urlbar {
            theme.color(ThemeRole::ModeToolbar)
        } else if focus() == Focus::Keyboard {
            theme.color(ThemeRole::FocusRing)
        } else if self.compact_sidebar {
            Color::TRANSPARENT
        } else {
            palette.urlbar_border
        };
        let border_width = if toolbar_cursor_on_urlbar {
            2.0
        } else if focus() == Focus::Keyboard {
            1.5
        } else {
            1.0
        };

        let open_bar_core = core.clone();
        let open_bar_url = active_url.clone();
        let open_bar = move || {
            let initial = if open_bar_url.is_empty() || open_bar_url == "about:blank" || open_bar_url == "about:newtab"
            {
                None
            } else {
                Some(open_bar_url.clone())
            };
            open_bar_core.invoke_or_log(Command::AddressBarOpen {
                new_tab: false,
                initial_query: initial,
            });
        };
        let open_bar_click = open_bar.clone();
        let open_bar_key = open_bar;

        let copy_core = core.clone();
        let show_copy = !floating_open && is_web_page && (!self.compact_sidebar || hovered() || site_panel_open);
        let copy_button = show_copy.then(|| {
            rect()
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Copy Current URL")
                .width(Size::px(22.0))
                .height(Size::px(22.0))
                .corner_radius(CornerRadius::new_all(5.0))
                .background(if copy_hovered() {
                    palette.surface_pill_hover
                } else {
                    Color::TRANSPARENT
                })
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(move |_| copy_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| copy_hovered.set_if_modified(false))
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    copy_core.invoke_or_log(Command::AddressBarCopyCurrentUrl);
                })
                .child(Icon::new("link").size(12.5).role(ThemeRole::TextMuted))
        });

        let site_core = core.clone();
        let site_btn_bg = if site_panel_open || site_btn_hovered() {
            palette.surface_pill_hover
        } else {
            Color::TRANSPARENT
        };
        // Matches Zen's `#zen-site-data-icon-button`: present at the right end of the URL bar in both single-toolbar
        // and multiple-toolbar layouts so the user always has a direct button for Site Data and Extensions.
        let site_button = (!floating_open).then(|| {
            rect()
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Site Data and Extensions")
                .width(Size::px(22.0))
                .height(Size::px(22.0))
                .corner_radius(CornerRadius::new_all(5.0))
                .background(site_btn_bg)
                .center()
                .cursor(CursorIcon::Pointer)
                .on_sized(move |e: Event<SizedEventData>| {
                    if let Some(state) = addr_state {
                        let mut anchor = state.site_panel_anchor;
                        anchor.set_if_modified(AnchorRect::from_area(e.area));
                    }
                })
                .on_pointer_enter(move |_| site_btn_hovered.set_if_modified(true))
                .on_pointer_leave(move |_| site_btn_hovered.set_if_modified(false))
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    site_core.invoke_or_log(Command::SitePanelToggle);
                })
                .child(Icon::new("sliders").size(12.5).role(if site_panel_open {
                    ThemeRole::Text
                } else {
                    ThemeRole::TextMuted
                }))
        });

        let id_core = core.clone();
        let identity_icon = (!floating_open).then(|| {
            let role = if site_info.security.is_warning() {
                ThemeRole::Danger
            } else {
                ThemeRole::TextMuted
            };
            rect()
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(format!("Site Identity: {}", site_info.security.label()))
                .width(Size::px(20.0))
                .height(Size::px(20.0))
                .corner_radius(CornerRadius::new_all(5.0))
                .center()
                .cursor(CursorIcon::Pointer)
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    id_core.invoke_or_log(Command::SitePanelToggle);
                })
                .child(Icon::new(site_info.security.toolbar_icon()).size(13.0).role(role))
        });

        let left_copy_button = (!self.compact_sidebar).then_some(copy_button.clone()).flatten();
        let right_copy_button = self.compact_sidebar.then_some(copy_button).flatten();
        let url_font_size = if self.compact_sidebar { 12.0 } else { 12.5 };
        let url_text_color = if is_placeholder {
            palette.text_muted
        } else {
            palette.text
        };

        let container = rect()
            .a11y_id(self.location_a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::TextInput)
            .a11y_alt("Address bar")
            .width(Size::fill())
            .height(Size::px(height))
            .padding(Gaps::new(0.0, 6.0, 0.0, 6.0))
            .spacing(4.0)
            .font_size(url_font_size)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .corner_radius(CornerRadius::new_all(radius))
            .background(bg)
            .border(
                Border::new()
                    .width(border_width)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_color),
            )
            .cursor(CursorIcon::Text)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |_| {
                open_bar_click();
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                let is_space = matches!(&e.key, Key::Character(ch) if ch == " ");
                if matches!(e.key, Key::Named(NamedKey::Enter)) || is_space {
                    e.stop_propagation();
                    open_bar_key();
                }
            })
            .maybe_child(left_copy_button)
            .maybe_child(identity_icon)
            .child(
                crate::ui::chrome::fade_text::FadeText::new(
                    display_label,
                    url_font_size,
                    FontWeight::MEDIUM,
                    url_text_color,
                )
                .fade_px(22.0),
            )
            .maybe_child(right_copy_button)
            .maybe_child(site_button);

        if self.compact_sidebar {
            container
        } else {
            container.max_width(Size::px(640.0))
        }
    }
}

/// Width of the three compact window buttons in the sidebar's top row, with the space between them.
pub const COMPACT_WINDOW_CONTROLS_WIDTH: f32 = 3.0 * 19.0 + 2.0;
/// Room the sidebar's top row reserves for the window buttons when deciding what moves into its `»` menu. It is what
/// the fork's buttons take, so the same buttons overflow at the same widths.
const OVERFLOW_WINDOW_CONTROLS_RESERVE: f32 = 98.0;

/// Minimize, maximize/restore, and close window buttons.
#[derive(Clone, Copy, PartialEq)]
pub struct WindowControls {
    pub interactive: bool,
    pub compact: bool,
}

impl Component for WindowControls {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let palette = ChromePalette::for_scheme(theme.scheme);
        let interactive = self.interactive;
        let btn_w = if self.compact { 19.0 } else { 26.0 };
        let btn_h = if self.compact { 24.0 } else { 26.0 };
        let icon_px = if self.compact { 9.5 } else { 11.0 };

        rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(if self.compact { 1.0 } else { 2.0 })
            .child(WindowControlButton {
                item_id: "builtin.window_minimize",
                label: "Minimize window",
                svg: ICON_MINIMIZE_SVG,
                svg_key: "zendia-win-min",
                width: btn_w,
                height: btn_h,
                icon_size: icon_px,
                hover_bg: palette.surface_pill,
                icon_color: palette.text_muted,
                hover_icon_color: palette.text,
                on_press: EventHandler::new(move |_| {
                    if interactive {
                        Platform::get().with_window(Platform::window_id(), |window| {
                            window.set_minimized(true);
                        });
                    }
                }),
            })
            .child(WindowControlButton {
                item_id: "builtin.window_maximize",
                label: "Maximize or restore window",
                svg: ICON_MAXIMIZE_SVG,
                svg_key: "zendia-win-max",
                width: btn_w,
                height: btn_h,
                icon_size: icon_px,
                hover_bg: palette.surface_pill,
                icon_color: palette.text_muted,
                hover_icon_color: palette.text,
                on_press: EventHandler::new(move |_| {
                    if interactive {
                        Platform::get().with_window(Platform::window_id(), |window| {
                            window.set_maximized(!window.is_maximized());
                        });
                    }
                }),
            })
            .child(WindowControlButton {
                item_id: "builtin.window_close",
                label: "Close window",
                svg: ICON_CLOSE_SVG,
                svg_key: "zendia-win-close",
                width: btn_w,
                height: btn_h,
                icon_size: icon_px,
                hover_bg: palette.window_close_hover,
                icon_color: palette.text_muted,
                hover_icon_color: if theme.scheme == ResolvedScheme::Light {
                    Color::WHITE
                } else {
                    palette.text
                },
                on_press: EventHandler::new(move |_| {
                    if interactive {
                        Platform::get().close_window(Platform::window_id());
                    }
                }),
            })
    }
}

#[derive(Clone, PartialEq)]
struct WindowControlButton {
    item_id: &'static str,
    label: &'static str,
    svg: &'static [u8],
    svg_key: &'static str,
    width: f32,
    height: f32,
    icon_size: f32,
    hover_bg: Color,
    icon_color: Color,
    hover_icon_color: Color,
    on_press: EventHandler<Event<PressEventData>>,
}

impl Component for WindowControlButton {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let feedback = use_interactive_feedback(true);
        let on_press = self.on_press.clone();
        let bg = feedback.background(Color::TRANSPARENT, self.hover_bg, self.hover_bg);
        let focus_border = {
            let model = core.model().read();
            apply_toolbar_cursor_border(
                Border::new().width(0.0).fill(Color::TRANSPARENT),
                &model,
                self.item_id,
                &theme,
            )
        };

        KitTooltip::new(self.label).child(
            rect()
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(self.label)
                .width(Size::px(self.width))
                .height(Size::px(self.height))
                .corner_radius(CornerRadius::new_all(6.0))
                .background(bg)
                .border(focus_border)
                .scale(feedback.scale())
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(feedback.on_pointer_enter())
                .on_pointer_leave(feedback.on_pointer_leave())
                .on_pointer_down(feedback.on_pointer_down())
                .on_global_pointer_up(feedback.on_pointer_up())
                .on_press(move |e| on_press.call(e))
                .child(
                    SvgViewer::new((self.svg_key, Bytes::from_static(self.svg)))
                        .width(Size::px(self.icon_size))
                        .height(Size::px(self.icon_size))
                        .color(if feedback.is_hovered() {
                            self.hover_icon_color
                        } else {
                            self.icon_color
                        })
                        .show_loader(false),
                ),
        )
    }
}

/// Full-width top toolbar rendered in multiple-toolbar and collapsed-sidebar layouts.
#[derive(Clone, PartialEq)]
pub struct TopToolbar {
    pub slots: ToolbarSlots,
    pub location: State<String>,
    pub location_a11y_id: AccessibilityId,
    pub interactive_window_controls: bool,
}

impl Component for TopToolbar {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let mut bar_width = use_state(|| 1200.0f32);
        let layout = core.model().read().layout.clone();
        let show_controls = show_window_controls_in_top_bar(&layout);
        let location = self.location;
        let location_a11y_id = self.location_a11y_id;
        let narrow_top_bar = *bar_width.read() < 720.0;

        let render_item = move |item: zendia_browser_core::contributions::ToolbarItem| {
            ToolbarItemView::new(item.clone(), location, location_a11y_id)
                .hide_chat_label(narrow_top_bar)
                .key(item.id.as_str().to_string())
                .into_element()
        };

        // Over a collapsed left sidebar the Compact Mode button is centered above the icon rail; everywhere else the
        // bar's leading buttons start at its left edge, whichever side the sidebar is on.
        let over_rail =
            layout.sidebar_side == SidebarSide::Left && !layout.sidebar_expanded && !layout.compact_hides_sidebar();
        let (rail_items, start_items): (Vec<_>, Vec<_>) = self.slots.top_bar_start.iter().cloned().partition(|item| {
            over_rail && item.placement == zendia_browser_core::contributions::ToolbarPlacement::SidebarTop
        });
        let left_cluster = rect()
            .height(Size::fill())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .maybe_child((!rail_items.is_empty()).then(|| {
                rect()
                    .width(Size::px(layout.effective_sidebar_width()))
                    .height(Size::fill())
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::center())
                    .cross_align(Alignment::center())
                    .children(rail_items.into_iter().map(render_item))
            }))
            .child(
                rect()
                    .padding(Gaps::new(0.0, 0.0, 0.0, if over_rail { 6.0 } else { 8.0 }))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(2.0)
                    .children(start_items.into_iter().map(render_item)),
            );

        let center_items = self.slots.top_bar_center.iter().cloned().map(render_item);
        let end_items = self.slots.top_bar_end.iter().cloned().map(render_item);

        let bar = rect()
            .width(Size::fill())
            .height(Size::px(TOP_TOOLBAR_HEIGHT))
            .on_sized(move |e: Event<SizedEventData>| {
                let w = e.area.size.width;
                if w > 0.0 && (w - *bar_width.peek()).abs() > 1.0 {
                    bar_width.set(w);
                }
            })
            .padding(Gaps::new(0.0, 8.0, 0.0, 0.0))
            .spacing(8.0)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .child(left_cluster)
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .height(Size::fill())
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::center())
                    .cross_align(Alignment::center())
                    .padding(Gaps::new(0.0, 12.0, 0.0, 12.0))
                    .children(center_items),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(4.0)
                    .children(end_items)
                    .maybe_child(show_controls.then(|| {
                        rect().padding(Gaps::new(0.0, 0.0, 0.0, 4.0)).child(WindowControls {
                            interactive: self.interactive_window_controls,
                            compact: false,
                        })
                    })),
            );

        if self.interactive_window_controls {
            bar.window_drag()
        } else {
            bar
        }
    }
}

/// In-flow collapsible top bar rendered in single-toolbar compact mode (`vertical-tabs-topbar.inc.css`,
/// `ZendiaChatButton.sys.mjs`, `ZendiaAgentsButton.sys.mjs`).
///
/// Rests at `ELEMENT_SEPARATION` (`8px`) and expands to `COMPACT_TOP_BAR_HEIGHT` (`34px`) when the pointer reaches
/// the top edge or a popup/focus holds it open, pushing the page down smoothly. Holds Agents, then Chat, before the
/// window controls at the trailing end.
#[derive(Clone, PartialEq)]
pub struct CompactTopBar {
    pub slots: ToolbarSlots,
    pub location: State<String>,
    pub location_a11y_id: AccessibilityId,
    pub interactive_window_controls: bool,
    /// Reveal animation progress in `0.0..=1.0`.
    pub revealed: f32,
}

impl Component for CompactTopBar {
    fn render(&self) -> impl IntoElement {
        let t = self.revealed.clamp(0.0, 1.0);
        let height = ELEMENT_SEPARATION + (COMPACT_TOP_BAR_HEIGHT - ELEMENT_SEPARATION) * t;
        let location = self.location;
        let location_a11y_id = self.location_a11y_id;
        let interactive_content = if t > 0.1 { Interactive::Yes } else { Interactive::No };

        let end_items = self.slots.top_bar_end.iter().map(move |item| {
            ToolbarItemView::new(item.clone(), location, location_a11y_id)
                .key(format!("compact-topbar:{}", item.id.as_str()))
                .into_element()
        });

        let bar = rect()
            .width(Size::fill())
            .height(Size::px(height))
            .overflow(Overflow::Clip)
            .direction(Direction::Horizontal)
            .main_align(Alignment::End)
            .cross_align(Alignment::center())
            .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
            .child(
                rect()
                    .height(Size::px(COMPACT_TOP_BAR_HEIGHT))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .opacity(t)
                    .interactive(interactive_content)
                    .child(
                        rect()
                            .margin(Gaps::new(0.0, 6.0, 0.0, 0.0))
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(4.0)
                            .children(end_items),
                    )
                    .child(WindowControls {
                        interactive: self.interactive_window_controls,
                        compact: false,
                    }),
            );

        if self.interactive_window_controls {
            bar.window_drag()
        } else {
            bar
        }
    }
}

/// Top-right `8×8` status dot for Chat (`TB-01`) and Web chat (`TB-03`) buttons (`1.2 s` pulse while working/loading).
#[derive(Clone, PartialEq)]
struct ToolbarPulseDot {
    color: Color,
    pulsing: bool,
    left_offset: Option<f32>,
    key: DiffKey,
}

impl KeyExt for ToolbarPulseDot {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ToolbarPulseDot {
    fn render(&self) -> impl IntoElement {
        let pulse = use_animation_with_dependencies(&self.pulsing, |conf, active| {
            if *active {
                conf.on_creation(OnCreation::Run);
                conf.on_change(OnChange::Rerun);
                conf.on_finish(OnFinish::reverse());
            } else {
                conf.on_creation(OnCreation::Finish);
            }
            AnimNum::new(0.38, 1.0)
                .duration(motion::TOOLBAR_DOT_PULSE)
                .ease(Ease::InOut)
                .function(Function::Sine)
        });
        let alpha = if self.pulsing {
            pulse.get().value().clamp(0.38, 1.0)
        } else {
            1.0
        };
        let pos = match self.left_offset {
            Some(left) => Position::new_absolute().top(-1.0).left(left),
            None => Position::new_absolute().top(-1.0).right(-1.0),
        };
        rect()
            .position(pos)
            .width(Size::px(8.0))
            .height(Size::px(8.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .opacity(alpha)
            .background(self.color)
            .interactive(Interactive::No)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Top-right `15 px` count badge for the Agents button (`TB-02`), pulsing (`1.4 s`) when amber (`Warning`).
#[derive(Clone, PartialEq)]
struct ToolbarPulseBadge {
    text: String,
    bg: Color,
    fg: Color,
    pulsing: bool,
    key: DiffKey,
}

impl KeyExt for ToolbarPulseBadge {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ToolbarPulseBadge {
    fn render(&self) -> impl IntoElement {
        let pulse = use_animation_with_dependencies(&self.pulsing, |conf, active| {
            if *active {
                conf.on_creation(OnCreation::Run);
                conf.on_change(OnChange::Rerun);
                conf.on_finish(OnFinish::reverse());
            } else {
                conf.on_creation(OnCreation::Finish);
            }
            AnimNum::new(0.45, 1.0)
                .duration(motion::AGENTS_ATTENTION_PULSE)
                .ease(Ease::InOut)
                .function(Function::Sine)
        });
        let alpha = if self.pulsing {
            pulse.get().value().clamp(0.45, 1.0)
        } else {
            1.0
        };
        rect()
            .position(Position::new_absolute().top(-4.0).right(-4.0))
            .min_width(Size::px(15.0))
            .height(Size::px(15.0))
            .padding(Gaps::new(0.0, 4.0, 0.0, 4.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .opacity(alpha)
            .background(self.bg)
            .shadow(Shadow::new().y(1.0).blur(2.0).color(Color::from_argb(64, 0, 0, 0)))
            .center()
            .interactive(Interactive::No)
            .child(
                label()
                    .text(self.text.clone())
                    .font_size(9.0)
                    .font_weight(FontWeight::BOLD)
                    .font_family("monospace")
                    .color(self.fg),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zendia_browser_core::contributions::{ContributionId, ToolbarPlacement, ToolbarPlacementRules};

    fn make_item(id: &str) -> ToolbarItem {
        ToolbarItem {
            id: ContributionId::new(id),
            title: id.to_string(),
            icon: "plus".to_string(),
            command: "tab.new".to_string(),
            command_args: Vec::new(),
            tooltip: None,
            badge: None,
            placement: ToolbarPlacement::SidebarTop,
            placement_rules: ToolbarPlacementRules::from_placement(ToolbarPlacement::SidebarTop),
            kind: ToolbarItemKind::Button,
            order: 0,
            visible: true,
            enabled: true,
            source: ContributionSource::BuiltIn,
        }
    }

    #[test]
    fn single_right_narrow_sidebar_overflows_excess_top_buttons() {
        let start = vec![make_item("builtin.app_menu"), make_item("builtin.sidebar_toggle")];
        let end = vec![
            make_item("builtin.back"),
            make_item("builtin.forward"),
            make_item("builtin.reload"),
        ];
        // At 186px with window controls (Single + Right), 5 buttons + 3 caption buttons overflow into `>>`.
        let (vis_start, vis_end, overflow) = partition_sidebar_top_row(&start, &end, true, 186.0);
        assert!(!overflow.is_empty());
        assert_eq!(vis_start[0].id.as_str(), "builtin.app_menu");
        assert_eq!(vis_start.len() + vis_end.len() + overflow.len(), 5);

        // At the fork's narrowest right sidebar, its menu holds Reload and then Compact Mode.
        let (_, _, narrow) = partition_sidebar_top_row(&start, &end, true, 231.5);
        let narrow: Vec<_> = narrow.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(narrow, ["builtin.reload", "builtin.sidebar_toggle"]);

        // Wide sidebar (280px) keeps every button inline with zero overflow.
        let (w_start, w_end, w_overflow) = partition_sidebar_top_row(&start, &end, true, 280.0);
        assert!(w_overflow.is_empty());
        assert_eq!(w_start.len(), 2);
        assert_eq!(w_end.len(), 3);
    }
}
