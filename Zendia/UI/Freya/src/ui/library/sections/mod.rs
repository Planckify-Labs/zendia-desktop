/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The library's sections, one file each. Each draws only its body; the shell in [`super::Library`] draws the rail.

pub mod downloads;
pub mod history;
pub mod media;
pub mod space_routing;
pub mod spaces;

use zendia_browser_core::library::LibrarySection;

/// Width of a section's body next to the rail.
pub fn width(section: LibrarySection) -> f32 {
    match section {
        LibrarySection::History => history::WIDTH,
        LibrarySection::Downloads => downloads::WIDTH,
        LibrarySection::Media => media::WIDTH,
        LibrarySection::Spaces => spaces::WIDTH,
        LibrarySection::SpaceRouting => space_routing::WIDTH,
    }
}
