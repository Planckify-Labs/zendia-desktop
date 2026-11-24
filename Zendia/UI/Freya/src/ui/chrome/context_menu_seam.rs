/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Seam where vertical tab right-click context menus attach to the sidebar frame.

use freya::prelude::CursorPoint;
use zendia_browser_core::model::TabId;

/// Claude's phase 1 menus fill this in.
pub fn open_tab_context_menu(_tab_id: TabId, _position: CursorPoint) {}
