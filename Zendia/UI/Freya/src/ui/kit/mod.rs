/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Shared UI components used by built-in surfaces and rendered for declarative plugin trees.

#![allow(dead_code, unused_imports)]

pub mod controls;
pub mod declared;
pub mod focus_trap;
pub mod icons;
pub mod interactive;
pub mod key_label;
pub mod library_sprite;
pub mod list_nav;
pub mod list_row;
pub mod loading;
pub mod overlay;
pub mod panel;
pub mod picker;
pub mod popup_presence;
pub mod text_field;
pub mod tooltip;
pub mod transitions;

pub use controls::{IconButton, KitCheckbox, KitChip, KitSlider, KitSwitch};
pub use declared::{DeclaredUi, command_from_action, render_ui_node};
pub use focus_trap::{focus_ring_border, handle_modal_tab_key, next_focus_trap_index};
pub use icons::{ICON_TABLE, Icon, canonical_icon_name, icon_raw_bytes, prepare_svg_for_rendering};
pub use interactive::{InteractiveFeedback, mix_color, press_scale_for, use_interactive_feedback};
pub use key_label::{KeyCap, KeyLabel, parse_key_notation};
pub use library_sprite::LibrarySprite;
pub use list_nav::{
    ListNavAction, ListNavConfig, TYPEAHEAD_TIMEOUT, TypeaheadState, classify_list_key, compute_scroll_into_view_y,
    first_selectable, last_selectable, moved_index, page_step_selectable, scroll_item_into_view, step_selectable,
};
pub use list_row::ListRow;
pub use loading::{IndeterminateBar, Spinner};
pub use overlay::{AnchoredPopover, ConfirmationCard, Toast};
pub use panel::{GlassPanel, Pill};
pub use picker::{PickerGrid, PickerItem, PickerList};
pub use popup_presence::{
    PopupContent, PopupHorizontalAlign, PopupPlacement, PopupPresence, StaggeredRow, anchor_popup_origin,
    clamp_popup_origin, delayed_ease_out, max_popup_height, popup_transform, use_last_some,
};
pub use text_field::{
    TextField, TextFocusTracker, delete_line_back_range, delete_word_back_range, is_text_editing_key,
    try_use_text_focus, use_provide_text_focus,
};
pub use tooltip::{KitTooltip, estimate_tooltip_size, split_tooltip_shortcut};
pub use transitions::{FadeTransition, SlideDirection, SlideTransition};
