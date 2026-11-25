/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! View-local state for the share confirmation dialog.

use freya::prelude::*;
use zendia_browser_core::share::SharePayload;

/// View-local state tracking an open share confirmation dialog and its form inputs.
#[derive(Clone, Copy, PartialEq)]
pub struct ShareViewState {
    pub payload: State<Option<SharePayload>>,
    pub name_draft: State<String>,
    pub dont_ask: State<bool>,
}

impl ShareViewState {
    /// Opens the share confirmation dialog seeded from `payload`.
    pub fn open(&self, payload: SharePayload) {
        let mut name = self.name_draft;
        name.set(payload.initial_name.clone());
        let mut dont_ask = self.dont_ask;
        dont_ask.set(payload.initial_dont_ask);
        let mut p = self.payload;
        p.set(Some(payload));
    }

    /// Closes the share confirmation dialog.
    pub fn close(&self) {
        let mut p = self.payload;
        p.set_if_modified(None);
    }

    /// Whether the share confirmation dialog is currently open.
    pub fn is_open(&self) -> bool {
        self.payload.peek().is_some()
    }
}

/// Installs [`ShareViewState`] into Freya context for the current window.
pub fn use_provide_share_state() -> ShareViewState {
    let payload = use_state(|| None::<SharePayload>);
    let name_draft = use_state(String::new);
    let dont_ask = use_state(|| false);

    use_provide_context(|| ShareViewState {
        payload,
        name_draft,
        dont_ask,
    })
}

/// Consumes [`ShareViewState`] if installed in this window.
pub fn try_use_share_state() -> Option<ShareViewState> {
    try_consume_context::<ShareViewState>()
}
