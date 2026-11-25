/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Vim mode overlays: statusline and mascot, which-key popup, hint labels, picker, and focus rings.

pub mod picker;
pub mod region_picker;

pub use picker::{
    VimPickerCard, VimPickerOverlay, VimPickerViewState, try_use_vim_picker_state, use_provide_vim_picker_state,
    use_vim_picker_state,
};
pub use region_picker::{
    RegionPickerOverlay, RegionPickerSurface, RegionPickerViewState, try_use_region_picker_state,
    use_provide_region_picker_state, use_region_picker_state,
};
