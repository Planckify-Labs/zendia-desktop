/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Page surfaces, multi-view split layouts, overlay layers, Glance previews, compact mode, and media controls.

pub mod download_arc;
pub mod glance;
pub mod keymap_context;
pub mod overlay;
pub mod page_area;
pub mod surface;
pub mod surface_store;

#[allow(unused_imports)]
pub use keymap_context::{KeymapController, use_keymap, use_provide_keymap};
#[allow(unused_imports)]
pub use overlay::{DebugInputOverlay, PageOverlayLayer, PageStage};
#[allow(unused_imports)]
pub use page_area::{PageArea, PaneMode, PlaceholderPage};
#[allow(unused_imports)]
pub use surface::{PageSurface, SurfaceTarget};
#[allow(unused_imports)]
pub use surface_store::{
    PageHubSink, PageSurfaceContext, PageSurfaceData, PageSurfaceTracker, SurfaceRect, use_page_surfaces,
    use_provide_page_surfaces,
};
