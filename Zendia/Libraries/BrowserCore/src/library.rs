/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The library: the window-filling panel that opens over the sidebar with Zen's sections for history, downloads,
//! media, spaces and space routing. Which section shows and whether the library is open are view state, so the
//! library's commands only ask the invoking window to open, close or switch it.

/// One of the library's sections, in the order its rail draws them from top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LibrarySection {
    SpaceRouting,
    Media,
    Downloads,
    Spaces,
    History,
}

impl LibrarySection {
    /// Every section, in rail order.
    pub const ALL: [LibrarySection; 5] = [
        LibrarySection::SpaceRouting,
        LibrarySection::Media,
        LibrarySection::Downloads,
        LibrarySection::Spaces,
        LibrarySection::History,
    ];

    /// The section the library opens on when none was asked for and none was shown before (Zen's default).
    pub const DEFAULT: LibrarySection = LibrarySection::History;

    /// The id used in command arguments and keymaps (`library.open section=history`), as the fork names them.
    pub fn id(self) -> &'static str {
        match self {
            LibrarySection::SpaceRouting => "space-routing",
            LibrarySection::Media => "media",
            LibrarySection::Downloads => "downloads",
            LibrarySection::Spaces => "spaces",
            LibrarySection::History => "history",
        }
    }

    /// The label under the section's rail icon.
    pub fn title(self) -> &'static str {
        match self {
            LibrarySection::SpaceRouting => "Routing",
            LibrarySection::Media => "Media",
            LibrarySection::Downloads => "Downloads",
            LibrarySection::Spaces => "Spaces",
            LibrarySection::History => "History",
        }
    }

    /// Parses a section id. `routing` is accepted as a short form of `space-routing`.
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "space-routing" | "routing" => Some(LibrarySection::SpaceRouting),
            _ => Self::ALL.into_iter().find(|section| section.id() == id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for section in LibrarySection::ALL {
            assert_eq!(LibrarySection::from_id(section.id()), Some(section));
        }
        assert_eq!(LibrarySection::from_id("routing"), Some(LibrarySection::SpaceRouting));
        assert_eq!(LibrarySection::from_id("boosts"), None);
    }
}
