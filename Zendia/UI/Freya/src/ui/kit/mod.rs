/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Shared UI components used by built-in surfaces and rendered for declarative plugin trees.

#![allow(dead_code, unused_imports)]

pub mod controls;
pub mod declared;
pub mod icons;
pub mod key_label;
pub mod list_row;
pub mod overlay;
pub mod panel;
pub mod picker;
pub mod transitions;

pub use controls::{IconButton, KitChip, KitSlider, KitSwitch};
pub use declared::{DeclaredUi, command_from_action, render_ui_node};
pub use icons::{ICON_TABLE, Icon, canonical_icon_name, icon_raw_bytes, prepare_svg_for_rendering};
pub use key_label::{KeyCap, KeyLabel, parse_key_notation};
pub use list_row::ListRow;
pub use overlay::{AnchoredPopover, ConfirmationCard, Toast};
pub use panel::{GlassPanel, Pill};
pub use picker::{PickerGrid, PickerItem, PickerList};
pub use transitions::{FadeTransition, SlideDirection, SlideTransition};
