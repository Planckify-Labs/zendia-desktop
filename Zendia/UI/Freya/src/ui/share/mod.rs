/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 12 share surfaces: centered share confirmation modal dialog (`zen-share-confirm`) and its view state.

pub mod confirm_dialog;
pub mod state;

pub use confirm_dialog::{ShareConfirmDialog, ShareConfirmOverlay};
pub use state::{ShareViewState, try_use_share_state, use_provide_share_state};
