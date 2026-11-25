/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Window shell chrome: window layout, toolbar layouts, vertical sidebar frame, menus and the about dialog.

pub mod about;
pub mod address_bar;
pub mod layout;
pub mod menus;
pub mod sidebar;
pub mod toasts;
pub mod toolbar;
pub mod window;

#[allow(unused_imports)]
pub use address_bar::{AddressBarLayer, AddressBarMode, AddressBarViewState, use_provide_address_bar_state};
#[allow(unused_imports)]
pub use layout::{ChromePalette, ToolbarSlots};
#[allow(unused_imports)]
pub use sidebar::{SidebarFrame, SidebarResizeHandle};
#[allow(unused_imports)]
pub use toolbar::{LocationBar, ToolbarItemView, TopToolbar, WindowControls};
#[allow(unused_imports)]
pub use window::WindowShell;
