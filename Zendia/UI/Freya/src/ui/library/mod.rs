/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Library panel for spaces, downloads, browsing history, space routing, and media previews.
//!
//! The library takes the sidebar's place in the window: a rail of section buttons and the active section's body,
//! with the page card pushed aside. Whether it is open and which section shows are view state in
//! [`LibraryViewState`]; the `library.*` commands reach it as view requests.
//!
//! Each section lives in its own file under [`sections`] and draws only its body. [`nav`] is the keyboard layer's
//! seam: sections register the rows a cursor can walk, and wrap each row in [`nav::LibraryCursorRing`].
//! [`search::LibrarySearchBar`] is the search box and filter button that History and Downloads share.

pub mod media_preview;
pub mod nav;
pub mod search;
pub mod sections;

use crate::ui::chrome::menus::{OpenContextMenu, try_use_menu_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::icons::{Icon, icon_raw_bytes};
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::events::ViewRequest;
use zendia_browser_core::history::build_history_tab_context_menu;
use zendia_browser_core::library::LibrarySection;
use zendia_browser_core::model::SidebarSide;

/// Width of the section rail on the library's window edge (`--zen-library-side` min-width 110 px).
pub const RAIL_WIDTH: f32 = 112.0;
/// Open/close slide duration matching Zen's spring (`stiffness: 720, damping: 47, mass: 1.2`).
pub const LIBRARY_SLIDE_DURATION: Duration = Duration::from_millis(300);
/// Section body fade-in duration when switching sections (`zen-library-section-fade 0.1s`).
pub const SECTION_FADE_DURATION: Duration = Duration::from_millis(100);
/// Minimum number of rail sections that must remain visible while customizing.
const MIN_VISIBLE_SECTIONS: usize = 2;

/// Per-window view state of the library.
#[derive(Clone, Copy, PartialEq)]
pub struct LibraryViewState {
    /// Whether the library is open in this window.
    pub open: State<bool>,
    /// The section the library shows, kept while it is closed so it reopens there.
    pub section: State<LibrarySection>,
    /// The keyboard layer's cursor and the rows the active section registered (see [`nav`]).
    pub nav: nav::LibraryNavState,
    /// Whether rail customize mode (hide/show sections) is active.
    pub customizing: State<bool>,
    /// Sections hidden from the rail in customize mode (at least two stay visible).
    pub hidden_sections: State<Vec<LibrarySection>>,
    /// Optional fixed slide progress (`0.0..=1.0`) for deterministic gallery previews of mid-open motion.
    pub pinned_progress: State<Option<f32>>,
    /// Optional History search query pre-populated by a gallery fixture.
    pub history_query_override: State<Option<String>>,
    /// Optional History filter-panel open state pre-populated by a gallery fixture.
    pub history_filters_open_override: State<Option<bool>>,
    /// Forces the teal vim focus ring on the active section's search box (used in gallery vim-focus preview).
    pub force_search_focus_ring: State<bool>,
}

impl LibraryViewState {
    pub fn open(&self, section: Option<LibrarySection>) {
        if let Some(section) = section {
            let mut current = self.section;
            current.set_if_modified(section);
        }
        let mut open = self.open;
        open.set_if_modified(true);
        let mut focused = self.nav.keyboard_focused;
        focused.set_if_modified(true);
        self.nav.reset_pending();
    }

    pub fn close(&self) {
        let mut open = self.open;
        open.set_if_modified(false);
        let mut customizing = self.customizing;
        customizing.set_if_modified(false);
        let mut focused = self.nav.keyboard_focused;
        focused.set_if_modified(false);
        self.nav.reset_pending();
    }

    /// Zen's toggle: asking for the section already showing closes the library; asking for another switches to it.
    pub fn toggle(&self, section: Option<LibrarySection>) {
        let is_open = *self.open.peek();
        let showing = *self.section.peek();
        match section {
            Some(section) if is_open && section != showing => self.open(Some(section)),
            _ if is_open => self.close(),
            _ => self.open(section),
        }
    }

    /// Applies one of the library's view requests. Returns false for requests that are not the library's.
    pub fn handle_view_request(&self, request: &ViewRequest) -> bool {
        match request {
            ViewRequest::LibraryToggle { section } => self.toggle(*section),
            ViewRequest::LibraryOpen { section } => self.open(*section),
            ViewRequest::LibraryClose => self.close(),
            _ => return false,
        }
        true
    }

    pub fn is_open(&self) -> bool {
        *self.open.read()
    }

    /// Total width (`RAIL_WIDTH + section body width`) for the currently active section.
    pub fn total_width(&self) -> f32 {
        RAIL_WIDTH + sections::width(*self.section.read())
    }

    /// Visible sections in rail order.
    pub fn visible_sections(&self) -> Vec<LibrarySection> {
        let hidden = self.hidden_sections.peek();
        LibrarySection::ALL
            .into_iter()
            .filter(|section| !hidden.contains(section))
            .collect()
    }

    /// Returns the next (`delta > 0`) or previous (`delta < 0`) visible section, wrapping around.
    pub fn adjacent_visible_section(&self, delta: isize) -> Option<LibrarySection> {
        let visible = self.visible_sections();
        if visible.is_empty() {
            return None;
        }
        let active = *self.section.peek();
        let idx = visible.iter().position(|s| *s == active).unwrap_or(0);
        let next = (idx as isize + delta).rem_euclid(visible.len() as isize) as usize;
        visible.get(next).copied()
    }

    /// Toggles rail customize mode.
    pub fn toggle_customizing(&self) {
        let mut customizing = self.customizing;
        let next = !*customizing.peek();
        customizing.set(next);
    }

    /// Hides or restores a rail section while guaranteeing at least [`MIN_VISIBLE_SECTIONS`] remain visible.
    pub fn toggle_section_hidden(&self, target: LibrarySection) {
        let mut hidden = self.hidden_sections;
        let mut list = hidden.peek().clone();
        if let Some(pos) = list.iter().position(|&s| s == target) {
            list.remove(pos);
        } else {
            let visible_count = LibrarySection::ALL.len().saturating_sub(list.len());
            if visible_count <= MIN_VISIBLE_SECTIONS {
                return;
            }
            list.push(target);
        }
        let active = *self.section.peek();
        if list.contains(&active)
            && let Some(fallback) = LibrarySection::ALL.into_iter().find(|s| !list.contains(s))
        {
            let mut section = self.section;
            section.set(fallback);
        }
        hidden.set(list);
    }

    /// Hands keyboard focus back to the page when the user clicks outside the library.
    pub fn suspend_keyboard_focus(&self) {
        let mut focused = self.nav.keyboard_focused;
        focused.set_if_modified(false);
        self.nav.reset_pending();
    }

    /// Reclaims keyboard focus when the user clicks inside the library.
    pub fn resume_keyboard_focus(&self) {
        if *self.open.peek() {
            let mut focused = self.nav.keyboard_focused;
            focused.set_if_modified(true);
        }
    }

    /// Resets gallery-only overrides so switching gallery states starts cleanly.
    pub fn clear_gallery_overrides(&self) {
        let mut pinned = self.pinned_progress;
        pinned.set_if_modified(None);
        let mut query = self.history_query_override;
        query.set_if_modified(None);
        let mut filters = self.history_filters_open_override;
        filters.set_if_modified(None);
        let mut ring = self.force_search_focus_ring;
        ring.set_if_modified(false);
    }
}

/// Installs [`LibraryViewState`] into Freya context for the current window.
pub fn use_provide_library_state() -> LibraryViewState {
    let open = use_state(|| false);
    let section = use_state(|| LibrarySection::DEFAULT);
    let nav = nav::use_library_nav_state();
    let customizing = use_state(|| false);
    let hidden_sections = use_state(Vec::<LibrarySection>::new);
    let pinned_progress = use_state(|| None::<f32>);
    let history_query_override = use_state(|| None::<String>);
    let history_filters_open_override = use_state(|| None::<bool>);
    let force_search_focus_ring = use_state(|| false);
    use_provide_context(|| LibraryViewState {
        open,
        section,
        nav,
        customizing,
        hidden_sections,
        pinned_progress,
        history_query_override,
        history_filters_open_override,
        force_search_focus_ring,
    })
}

/// Consumes [`LibraryViewState`] if installed in this window.
pub fn try_use_library_state() -> Option<LibraryViewState> {
    try_consume_context::<LibraryViewState>()
}

/// Canonical Zen icon name for a library rail section.
pub fn section_icon_name(section: LibrarySection) -> &'static str {
    match section {
        LibrarySection::SpaceRouting => "library-space-routing",
        LibrarySection::Media => "library-media",
        LibrarySection::Downloads => "library-downloads",
        LibrarySection::Spaces => "library-spaces",
        LibrarySection::History => "library-history",
    }
}

type LibraryIconCache = HashMap<(LibrarySection, bool, bool), Bytes>;

/// Prepares a Zen library icon with distinct stroke and fill paints: inactive tabs are outlined (`fill="none"`),
/// active tabs carry a softly tinted fill matching Zen's `color-mix(in srgb, var(--stroke), light-dark(white, black) 60%)`.
fn prepared_library_icon(section: LibrarySection, active: bool, scheme: ResolvedScheme) -> Option<(String, Bytes)> {
    static CACHE: std::sync::LazyLock<Mutex<LibraryIconCache>> =
        std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

    let is_dark = scheme.is_dark();
    let cache_key = (section, active, is_dark);
    let viewer_key = format!(
        "zendia-lib-icon-{}-{}-{}",
        section.id(),
        if active { "active" } else { "idle" },
        if is_dark { "dark" } else { "light" }
    );

    if let Ok(guard) = CACHE.lock()
        && let Some(bytes) = guard.get(&cache_key)
    {
        return Some((viewer_key, bytes.clone()));
    }

    let raw = icon_raw_bytes(section_icon_name(section))?;
    let text = String::from_utf8_lossy(raw);
    let (stroke_hex, fill_paint) = match (scheme, active) {
        (ResolvedScheme::Dark, false) => ("#d4d8e8", "none"),
        (ResolvedScheme::Dark, true) => ("#e4e7f7", "#6f6c88"),
        (ResolvedScheme::Light, false) => ("#493e9c", "none"),
        (ResolvedScheme::Light, true) => ("#493e9c", "#b8b2de"),
    };
    let prepared = text
        .replace("context-fill-opacity", "1")
        .replace("context-stroke-opacity", "1")
        .replace("context-fill", fill_paint)
        .replace("context-stroke", stroke_hex);
    let bytes = Bytes::from(prepared.into_bytes());
    if let Ok(mut guard) = CACHE.lock() {
        guard.insert(cache_key, bytes.clone());
    }
    Some((viewer_key, bytes))
}

/// The library as it stands in the window: the rail and the active section's body. The window shell draws it in the
/// sidebar's place while [`LibraryViewState::open`] is set (or while its close slide finishes).
#[derive(Clone, PartialEq)]
pub struct Library;

impl Component for Library {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let state = try_use_library_state();

        let active = state.map(|s| *s.section.read()).unwrap_or(LibrarySection::DEFAULT);
        let customizing = state.is_some_and(|s| *s.customizing.read());
        let hidden = state.map(|s| s.hidden_sections.read().clone()).unwrap_or_default();
        let sidebar_side = core.model().read().layout.sidebar_side;
        let is_right = sidebar_side == SidebarSide::Right;

        // Zen's 100 ms fade when switching between sections.
        let section_fade = use_animation_with_dependencies(&active, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(SECTION_FADE_DURATION)
                .ease(Ease::InOut)
                .function(motion::ease_out_fn())
        });
        let body_opacity = section_fade.get().value().clamp(0.0, 1.0);

        let rail_gradient = {
            let edge_tint = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(255, 255, 255, 0.28).to_color(),
            };
            let angle = if is_right { 270.0 } else { 90.0 };
            LinearGradient::new()
                .angle(angle)
                .stop((Color::TRANSPARENT, 0.0))
                .stop((edge_tint, 1.0))
        };
        let rail_shadow = Shadow::new()
            .x(if is_right { -9.0 } else { 9.0 })
            .y(0.0)
            .blur(17.0)
            .color(Color::from_argb(14, 0, 0, 0));

        let visible_sections: Vec<LibrarySection> = LibrarySection::ALL
            .into_iter()
            .filter(|section| customizing || !hidden.contains(section))
            .collect();
        let can_hide_more = LibrarySection::ALL.len().saturating_sub(hidden.len()) > MIN_VISIBLE_SECTIONS;

        let mut tabs_column = rect().width(Size::fill()).cross_align(Alignment::center()).spacing(8.0);
        for section in visible_sections {
            let is_hidden = hidden.contains(&section);
            tabs_column = tabs_column.child(RailTabButton {
                section,
                selected: section == active,
                customizing,
                hidden_tab: is_hidden,
                can_hide: is_hidden || can_hide_more,
                key: DiffKey::from(&section.id()),
            });
        }

        let footer_buttons = {
            let close_core = core.clone();
            let donate_core = core.clone();
            let close_icon = if is_right { "forward" } else { "back" };

            let close_btn = FooterIconButton {
                icon: close_icon,
                alt: "Close library",
                active: false,
                on_press: EventHandler::new(move |_| {
                    close_core.invoke_or_log(Command::LibraryClose);
                }),
                key: DiffKey::from(&"footer-close"),
            };
            let customize_btn = FooterIconButton {
                icon: "paintbrush-fill",
                alt: "Choose tabs",
                active: customizing,
                on_press: EventHandler::new(move |_| {
                    if let Some(lib) = state {
                        lib.toggle_customizing();
                    }
                }),
                key: DiffKey::from(&"footer-customize"),
            };
            let donate_btn = FooterIconButton {
                icon: "heart-circle-fill",
                alt: "Donate to Zen",
                active: false,
                on_press: EventHandler::new(move |_| {
                    donate_core.invoke_or_log(Command::TabNew {
                        url: Some("https://www.zen-browser.app/donate".to_string()),
                        after: None,
                        activate: true,
                    });
                    donate_core.invoke_or_log(Command::LibraryClose);
                }),
                key: DiffKey::from(&"footer-donate"),
            };

            let mut row = rect()
                .width(Size::fill())
                .padding(Gaps::new(8.0, 10.0, 10.0, 10.0))
                .direction(Direction::Horizontal)
                .main_align(Alignment::SpaceBetween)
                .cross_align(Alignment::center());
            if is_right {
                row = row.child(donate_btn).child(customize_btn).child(close_btn);
            } else {
                row = row.child(close_btn).child(customize_btn).child(donate_btn);
            }
            row
        };

        let rail = rect()
            .width(Size::px(RAIL_WIDTH))
            .height(Size::fill())
            .background(rail_gradient)
            .shadow(rail_shadow)
            .main_align(Alignment::SpaceBetween)
            .cross_align(Alignment::center())
            // Top spacer balances the footer so the tabs sit optically centered just like Zen.
            .child(rect().width(Size::fill()).height(Size::px(42.0)))
            .child(tabs_column)
            .child(footer_buttons);

        let body_section = match active {
            LibrarySection::History => sections::history::HistorySection.into_element(),
            LibrarySection::Downloads => sections::downloads::DownloadsSection.into_element(),
            LibrarySection::Media => sections::media::MediaSection.into_element(),
            LibrarySection::Spaces => sections::spaces::SpacesSection.into_element(),
            LibrarySection::SpaceRouting => sections::space_routing::SpaceRoutingSection.into_element(),
        };

        let list_focus_id = state.map(|s| s.nav.list_focus_a11y_id);
        let body = rect()
            .width(Size::px(sections::width(active)))
            .height(Size::fill())
            .overflow(Overflow::Clip)
            .opacity(body_opacity)
            .child(body_section);

        let click_core = core.clone();
        let mut container = rect()
            .width(Size::px(RAIL_WIDTH + sections::width(active)))
            .height(Size::fill())
            .direction(Direction::Horizontal)
            .maybe(list_focus_id.is_some(), |r| {
                r.a11y_id(list_focus_id.unwrap())
                    .a11y_focusable(true)
                    .a11y_role(AccessibilityRole::ListBox)
                    .a11y_alt("Library")
            })
            .on_pointer_down(move |_| {
                if let Some(lib) = state {
                    lib.resume_keyboard_focus();
                }
                click_core.invoke_or_log(Command::LibraryFocusSet { focused: true });
            });

        if is_right {
            container = container.child(body).child(rail);
        } else {
            container = container.child(rail).child(body);
        }
        container
    }
}

#[derive(Clone, PartialEq)]
struct RailTabButton {
    section: LibrarySection,
    selected: bool,
    customizing: bool,
    hidden_tab: bool,
    can_hide: bool,
    key: DiffKey,
}

impl Component for RailTabButton {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let state = try_use_library_state();
        let menu_state = try_use_menu_state();
        let mut hovered = use_state(|| false);

        let bg = if self.selected {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.11).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.09).to_color(),
            }
        } else if *hovered.read() {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.05).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.045).to_color(),
            }
        } else {
            Color::TRANSPARENT
        };

        let tile_border = if self.customizing {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.35).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.32).to_color(),
            }
        } else if self.selected {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.05).to_color(),
            }
        } else {
            Color::TRANSPARENT
        };

        let icon_el: Element =
            if let Some((cache_key, bytes)) = prepared_library_icon(self.section, self.selected, theme.scheme) {
                SvgViewer::new((cache_key, bytes))
                    .width(Size::px(28.0))
                    .height(Size::px(28.0))
                    .show_loader(false)
                    .into_element()
            } else {
                Icon::new(section_icon_name(self.section))
                    .size(28.0)
                    .role(ThemeRole::Text)
                    .into_element()
            };

        let section = self.section;
        let customizing = self.customizing;
        let hidden_tab = self.hidden_tab;
        let can_hide = self.can_hide;
        let press_core = core.clone();

        let toggle_badge = customizing.then(|| {
            let badge_bg = match theme.scheme {
                ResolvedScheme::Dark => Color::from_rgb(58, 58, 62),
                ResolvedScheme::Light => Color::WHITE,
            };
            let badge_border = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.18).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.15).to_color(),
            };
            rect()
                .position(Position::new_absolute().top(-5.0).left(-5.0))
                .layer(Layer::Relative(4))
                .width(Size::px(18.0))
                .height(Size::px(18.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(badge_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(badge_border),
                )
                .shadow(Shadow::new().y(1.0).blur(4.0).color(Color::from_argb(64, 0, 0, 0)))
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .opacity(if can_hide { 1.0 } else { 0.45 })
                .cursor(if can_hide {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                })
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    if can_hide && let Some(lib) = state {
                        lib.toggle_section_hidden(section);
                    }
                })
                .child(
                    Icon::new(if hidden_tab { "plus" } else { "close" })
                        .size(9.0)
                        .role(ThemeRole::Text),
                )
        });

        rect()
            .a11y_role(AccessibilityRole::Tab)
            .a11y_alt(section.title())
            .width(Size::px(RAIL_WIDTH - 16.0))
            .padding(Gaps::new(15.0, 4.0, 15.0, 4.0))
            .corner_radius(CornerRadius::new_all(10.0))
            .background(bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(tile_border),
            )
            .opacity(if customizing && hidden_tab { 0.45 } else { 1.0 })
            .main_align(Alignment::center())
            .cross_align(Alignment::center())
            .spacing(5.0)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_mouse_down(move |e: Event<MouseEventData>| {
                if e.button == Some(MouseButton::Right)
                    && section == LibrarySection::History
                    && let Some(ms) = menu_state
                {
                    e.stop_propagation();
                    ms.open_context_menu(OpenContextMenu {
                        position: (e.global_location.x as f32, e.global_location.y as f32),
                        items: build_history_tab_context_menu(),
                        tab_id: None,
                        open_submenu: None,
                    });
                }
            })
            .on_press(move |_| {
                if customizing {
                    return;
                }
                press_core.invoke_or_log(Command::LibraryOpen { section: Some(section) });
            })
            .child(icon_el)
            .child(
                label()
                    .text(section.title())
                    .font_size(12.5)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .color(theme.color(ThemeRole::Text)),
            )
            .maybe_child(toggle_badge)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct FooterIconButton {
    icon: &'static str,
    alt: &'static str,
    active: bool,
    on_press: EventHandler<()>,
    key: DiffKey,
}

impl Component for FooterIconButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);

        let bg = if self.active {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.16).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.12).to_color(),
            }
        } else if *hovered.read() {
            match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.08).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.07).to_color(),
            }
        } else {
            Color::TRANSPARENT
        };

        let on_press = self.on_press.clone();
        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.alt)
            .width(Size::px(28.0))
            .height(Size::px(28.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .main_align(Alignment::center())
            .cross_align(Alignment::center())
            .cursor(CursorIcon::Pointer)
            .opacity(if self.active || *hovered.read() { 0.95 } else { 0.72 })
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |_| on_press.call(()))
            .child(Icon::new(self.icon).size(15.0).role(ThemeRole::Text))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
