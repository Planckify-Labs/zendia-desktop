/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The search box and filter button at the top of the History and Downloads sections (Zen's
//! `ZenLibrarySearchSection`). The section owns the query state and filters the core's data with it.

use freya::prelude::*;

#[derive(Clone, PartialEq)]
pub struct LibrarySearchBar {
    pub placeholder: String,
    pub query: State<String>,
}

impl LibrarySearchBar {
    pub fn new(placeholder: impl Into<String>, query: State<String>) -> Self {
        Self {
            placeholder: placeholder.into(),
            query,
        }
    }
}

impl Component for LibrarySearchBar {
    fn render(&self) -> impl IntoElement {
        rect().width(Size::fill()).padding(Gaps::new_all(8.0)).child(
            Input::new(self.query)
                .placeholder(self.placeholder.clone())
                .width(Size::fill()),
        )
    }
}
