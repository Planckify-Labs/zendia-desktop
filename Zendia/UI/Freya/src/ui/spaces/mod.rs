/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 11 spaces editing surfaces: sidebar space creation form, gradient generator theme picker popup, and horizontal
//! space swipe motion (including swiping past the last space to offer creating a new one).

pub mod create_form;
pub mod gradient_generator;
pub mod icon_catalog;
pub mod state;
pub mod swipe;

pub use create_form::SpaceCreationForm;
pub use gradient_generator::{GradientGeneratorCard, GradientGeneratorOverlay, SpaceTextureOverlay};
pub use state::{
    SpaceEditingState, SpaceSwipeMode, SpaceSwipeViewState, try_use_space_editing_state,
    use_provide_space_editing_state,
};
pub use swipe::{SpaceAddSwipeOverlay, SpaceLibrarySwipeOverlay, SpaceSwipeIndicator, handle_sidebar_wheel};
