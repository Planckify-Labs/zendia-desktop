/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The library's keyboard layer seam (Zendia's `ZendiaLibraryView`).
//!
//! A section calls [`use_library_nav`] at the top of its `render` with the rows a cursor can walk, in the order they
//! are drawn, and wraps each row in [`LibraryCursorRing`]. The keyboard layer moves the cursor over the registered
//! rows and, on Enter, invokes the row's command. Row keys are strings made from core ids (`history:42`,
//! `download:7`, `space:3`), never Freya ids, and they only need to be unique within one section.

use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::library::LibrarySection;

/// One row or tile the cursor can stop on.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryNavRow {
    pub key: String,
    /// What Enter does on this row, if anything.
    pub activate: Option<Command>,
}

impl LibraryNavRow {
    pub fn new(key: impl Into<String>, activate: Option<Command>) -> Self {
        Self {
            key: key.into(),
            activate,
        }
    }
}

/// How the registered rows are laid out, which decides what the arrow keys and h/j/k/l do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryNavLayout {
    List,
    /// Tiles in rows of `columns`, left to right.
    Grid {
        columns: usize,
    },
    /// Side-by-side columns (the Spaces section's cards), each its own list.
    Columns,
}

/// The rows the active section registered.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryNavRows {
    pub section: LibrarySection,
    pub layout: LibraryNavLayout,
    pub rows: Vec<LibraryNavRow>,
}

/// The keyboard layer's view state, part of [`super::LibraryViewState`].
#[derive(Clone, Copy, PartialEq)]
pub struct LibraryNavState {
    pub rows: State<Option<LibraryNavRows>>,
    /// Key of the row under the cursor.
    pub cursor: State<Option<String>>,
}

pub(super) fn use_library_nav_state() -> LibraryNavState {
    LibraryNavState {
        rows: use_state(|| None),
        cursor: use_state(|| None),
    }
}

/// Registers the active section's walkable rows. Call it once, unconditionally, at the top of the section's
/// `render`. Returns the key under the cursor.
pub fn use_library_nav(section: LibrarySection, layout: LibraryNavLayout, rows: Vec<LibraryNavRow>) -> Option<String> {
    let state = super::try_use_library_state();
    let next = LibraryNavRows { section, layout, rows };
    if let Some(state) = state {
        let mut registered = state.nav.rows;
        if registered.peek().as_ref() != Some(&next) {
            registered.set(Some(next));
        }
        return state.nav.cursor.read().clone();
    }
    None
}

/// Draws the keyboard cursor's ring around one row or tile when `focused`.
#[derive(Clone, PartialEq)]
pub struct LibraryCursorRing {
    pub focused: bool,
    pub corner_radius: f32,
    pub child: Element,
}

impl LibraryCursorRing {
    pub fn new(focused: bool, child: impl IntoElement) -> Self {
        Self {
            focused,
            corner_radius: 8.0,
            child: child.into_element(),
        }
    }

    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = radius;
        self
    }
}

impl Component for LibraryCursorRing {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let ring = if self.focused {
            theme.color(ThemeRole::FocusRing)
        } else {
            Color::TRANSPARENT
        };
        rect()
            .width(Size::fill())
            .corner_radius(CornerRadius::new_all(self.corner_radius))
            .border(Border::new().width(2.0).alignment(BorderAlignment::Inner).fill(ring))
            .child(self.child.clone())
    }
}
