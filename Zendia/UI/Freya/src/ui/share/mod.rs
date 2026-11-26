/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 12 share surfaces: centered share confirmation modal dialog (`zen-share-confirm`), incoming share preview
//! overlay (`zen-share-overlay`), and view state.

pub mod confirm_dialog;
pub mod receive_overlay;
pub mod state;

pub use confirm_dialog::{ShareConfirmDialog, ShareConfirmOverlay};
pub use receive_overlay::{ShareReceiveCard, ShareReceiveOverlay};
pub use state::{ShareViewState, try_use_share_state, use_provide_share_state};
