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

pub mod nav;
pub mod search;
pub mod sections;

use crate::ui::core_view::use_core;
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::events::ViewRequest;
use zendia_browser_core::library::LibrarySection;

/// Width of the section rail on the library's window edge.
pub const RAIL_WIDTH: f32 = 112.0;

/// Per-window view state of the library.
#[derive(Clone, Copy, PartialEq)]
pub struct LibraryViewState {
    /// Whether the library is open in this window.
    pub open: State<bool>,
    /// The section the library shows, kept while it is closed so it reopens there.
    pub section: State<LibrarySection>,
    /// The keyboard layer's cursor and the rows the active section registered (see [`nav`]).
    pub nav: nav::LibraryNavState,
}

impl LibraryViewState {
    pub fn open(&self, section: Option<LibrarySection>) {
        if let Some(section) = section {
            let mut current = self.section;
            current.set_if_modified(section);
        }
        let mut open = self.open;
        open.set_if_modified(true);
    }

    pub fn close(&self) {
        let mut open = self.open;
        open.set_if_modified(false);
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
}

/// Installs [`LibraryViewState`] into Freya context for the current window.
pub fn use_provide_library_state() -> LibraryViewState {
    let open = use_state(|| false);
    let section = use_state(|| LibrarySection::DEFAULT);
    let nav = nav::use_library_nav_state();
    use_provide_context(|| LibraryViewState { open, section, nav })
}

/// Consumes [`LibraryViewState`] if installed in this window.
pub fn try_use_library_state() -> Option<LibraryViewState> {
    try_consume_context::<LibraryViewState>()
}

/// The library as it stands in the window: the rail and the active section's body. The window shell draws it in the
/// sidebar's place while [`LibraryViewState::open`] is set.
#[derive(Clone, PartialEq)]
pub struct Library;

impl Component for Library {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let state = try_use_library_state();
        let active = state.map(|s| *s.section.read()).unwrap_or(LibrarySection::DEFAULT);

        let mut rail = rect()
            .width(Size::px(RAIL_WIDTH))
            .height(Size::fill())
            .main_align(Alignment::Center)
            .cross_align(Alignment::Center)
            .spacing(8.0);
        for section in LibrarySection::ALL {
            let core = core.clone();
            let selected = section == active;
            rail = rail.child(
                rect()
                    .width(Size::px(RAIL_WIDTH - 16.0))
                    .padding(Gaps::new_all(10.0))
                    .corner_radius(CornerRadius::new_all(8.0))
                    .cross_align(Alignment::Center)
                    .maybe(selected, |r| r.background(theme.color(ThemeRole::Hover)))
                    .on_press(move |_| {
                        core.invoke_or_log(Command::LibraryOpen { section: Some(section) });
                    })
                    .child(label().text(section.title()).color(theme.color(ThemeRole::Text))),
            );
        }

        let body = match active {
            LibrarySection::History => sections::history::HistorySection.into_element(),
            LibrarySection::Downloads => sections::downloads::DownloadsSection.into_element(),
            LibrarySection::Media => sections::media::MediaSection.into_element(),
            LibrarySection::Spaces => sections::spaces::SpacesSection.into_element(),
            LibrarySection::SpaceRouting => sections::space_routing::SpaceRoutingSection.into_element(),
        };

        rect()
            .height(Size::fill())
            .direction(Direction::Horizontal)
            .child(rail)
            .child(
                rect()
                    .width(Size::px(sections::width(active)))
                    .height(Size::fill())
                    .child(body),
            )
    }
}
