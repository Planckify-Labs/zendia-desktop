/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! View-local sidebar state: inline rename drafts, active drag-reorder preview, and gallery hover preview.
//!
//! Only transient UI state lives here. Tabs, spaces, essentials, pinned tabs, folders, and notifications live in
//! [`zendia_browser_core`].

use crate::ui::theme::{ResolvedScheme, ResolvedTheme, Rgba};
use freya::prelude::*;
use zendia_browser_core::model::{FolderId, TabId, TabMoveTarget};

/// Active tab drag-and-drop reorder state kept strictly in the Freya view.
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarDragState {
    pub tab_id: TabId,
    pub tab_title: String,
    pub tab_favicon: Option<String>,
    pub pointer_x: f32,
    pub pointer_y: f32,
    pub target: Option<TabMoveTarget>,
    /// When true, renders a deterministic static drag preview (used by the UI gallery).
    pub is_preview: bool,
}

/// View-local state shared across sidebar components for inline renaming and drag reordering.
#[derive(Clone, Copy, PartialEq)]
pub struct SidebarViewState {
    pub renaming_tab: State<Option<TabId>>,
    pub renaming_folder: State<Option<FolderId>>,
    pub rename_draft: State<String>,
    pub drag_state: State<Option<SidebarDragState>>,
    pub preview_hovered_tab: State<Option<TabId>>,
    pub preview_hovered_separator: State<bool>,
}

impl SidebarViewState {
    /// Begins inline renaming for `tab_id` seeded with its current display title.
    pub fn start_tab_rename(&self, tab_id: TabId, initial_title: String) {
        let mut folder = self.renaming_folder;
        folder.set_if_modified(None);
        let mut draft = self.rename_draft;
        draft.set(initial_title);
        let mut tab = self.renaming_tab;
        tab.set(Some(tab_id));
    }

    /// Begins inline renaming for `folder_id` seeded with its current name.
    pub fn start_folder_rename(&self, folder_id: FolderId, initial_name: String) {
        let mut tab = self.renaming_tab;
        tab.set_if_modified(None);
        let mut draft = self.rename_draft;
        draft.set(initial_name);
        let mut folder = self.renaming_folder;
        folder.set(Some(folder_id));
    }

    /// Cancels any active inline tab or folder rename without invoking a core command.
    pub fn cancel_rename(&self) {
        let mut tab = self.renaming_tab;
        tab.set_if_modified(None);
        let mut folder = self.renaming_folder;
        folder.set_if_modified(None);
    }

    /// Clears all transient sidebar view state (rename, drag, and gallery preview flags).
    pub fn reset(&self) {
        self.cancel_rename();
        let mut drag = self.drag_state;
        drag.set_if_modified(None);
        let mut hovered = self.preview_hovered_tab;
        hovered.set_if_modified(None);
        let mut sep = self.preview_hovered_separator;
        sep.set_if_modified(false);
    }

    /// Updates the drag drop target while a live drag is in progress.
    pub fn set_drag_target(&self, target: TabMoveTarget) {
        let mut drag = self.drag_state;
        let should_update = drag
            .peek()
            .as_ref()
            .is_some_and(|d| !d.is_preview && d.target != Some(target));
        if should_update && let Some(current) = drag.write().as_mut() {
            current.target = Some(target);
        }
    }
}

/// Installs [`SidebarViewState`] into Freya context for the current window.
pub fn use_provide_sidebar_state() -> SidebarViewState {
    let renaming_tab = use_state(|| None::<TabId>);
    let renaming_folder = use_state(|| None::<FolderId>);
    let rename_draft = use_state(String::new);
    let drag_state = use_state(|| None::<SidebarDragState>);
    let preview_hovered_tab = use_state(|| None::<TabId>);
    let preview_hovered_separator = use_state(|| false);

    use_provide_context(|| SidebarViewState {
        renaming_tab,
        renaming_folder,
        rename_draft,
        drag_state,
        preview_hovered_tab,
        preview_hovered_separator,
    })
}

/// Consumes [`SidebarViewState`] if installed in this window.
pub fn try_use_sidebar_state() -> Option<SidebarViewState> {
    try_consume_context::<SidebarViewState>()
}

/// Subtle colored badge tint for known sample favicon icon names, matching the crisp colored site favicons in the
/// reference screenshots while falling back to a neutral surface when a tab has no favicon.
pub fn favicon_badge_colors(favicon: Option<&str>, theme: &ResolvedTheme) -> (Color, Color) {
    let (badge_rgba, fg) = match favicon {
        Some("mail") => (Rgba::rgb(42, 166, 214), Color::WHITE),
        Some("code") => (Rgba::rgb(32, 174, 128), Color::WHITE),
        Some("sparkles") | Some("palette") => (Rgba::rgb(228, 72, 134), Color::WHITE),
        Some("flame") => (Rgba::rgb(236, 122, 36), Color::WHITE),
        Some("book") => (Rgba::rgb(52, 122, 232), Color::WHITE),
        Some("terminal") => (Rgba::rgb(112, 108, 212), Color::WHITE),
        Some("compass") => (Rgba::rgb(56, 152, 206), Color::WHITE),
        _ => {
            let bg = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.10),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.08),
            };
            let icon_fg = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.72).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(30, 34, 48, 0.72).to_color(),
            };
            return (bg.to_color(), icon_fg);
        }
    };
    (badge_rgba.to_color(), fg)
}
