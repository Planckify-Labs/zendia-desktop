/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Web chat side panel, site strip, brand emblems, persistent Hub view registry, and dictation indicator.

pub mod logos;
pub mod panel;
pub mod views;

pub use logos::{chat_site_logo, parse_hex_color};
pub use panel::WebChatPanel;
pub use views::{WebChatViewRegistry, try_use_webchat_views, use_provide_webchat_views};
