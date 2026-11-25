/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 3 address bar and command palette surfaces: floating address bar, Actions mode palette,
//! Site Data Panel, and the window-root [`AddressBarLayer`].

pub mod floating_bar;
pub mod site_panel;
pub mod state;

#[allow(unused_imports)]
pub use floating_bar::{FloatingAddressBarCard, FloatingAddressBarOverlay, zendia_mascot_mini_svg};
#[allow(unused_imports)]
pub use site_panel::{SiteDataPanelCard, SiteDataPanelOverlay, compute_site_panel_origin};
#[allow(unused_imports)]
pub use state::{
    AddressBarMode, AddressBarViewState, compute_floating_bar_geometry, step_highlight, try_use_address_bar_state,
    use_address_bar_state, use_provide_address_bar_state,
};

use freya::prelude::*;

/// Single overlay host placed once at the window root above chrome and page surfaces for the floating address bar
/// and the anchored Site Data Panel.
#[derive(Clone, PartialEq)]
pub struct AddressBarLayer;

impl Component for AddressBarLayer {
    fn render(&self) -> impl IntoElement {
        rect().child(FloatingAddressBarOverlay).child(SiteDataPanelOverlay)
    }
}
