/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Native first-run onboarding flow covering the Living Emblem start page, Vim keys, agents, uBlock shield, Move in
//! (import, search engine, default browser), Essential apps, and Go summary.

pub mod audio;
pub mod browser_scene;
pub mod logos;
pub mod mascot;
pub mod scenes;
pub mod theme;
pub mod view;

pub use view::{
    OnboardingOverlay, OnboardingView, OnboardingViewState, handle_onboarding_key_down, try_use_onboarding_state,
    use_onboarding_state, use_provide_onboarding_state,
};
