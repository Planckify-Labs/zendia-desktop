/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Window shell chrome: window layout, toolbar layouts, vertical sidebar frame, chrome view state, and menu seams.

pub mod context_menu_seam;
pub mod layout;
pub mod sidebar;
pub mod toolbar;
pub mod view_state;
pub mod window;

#[allow(unused_imports)]
pub use layout::{ChromePalette, ToolbarSlots};
#[allow(unused_imports)]
pub use sidebar::{SidebarFrame, SidebarResizeHandle};
#[allow(unused_imports)]
pub use toolbar::{LocationBar, ToolbarItemView, TopToolbar, WindowControls};
#[allow(unused_imports)]
pub use view_state::{
    ChromeViewState, try_use_chrome_view_state, use_chrome_view_state, use_provide_chrome_view_state,
};
#[allow(unused_imports)]
pub use window::WindowShell;
