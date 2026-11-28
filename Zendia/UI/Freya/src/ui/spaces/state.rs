/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! View-local state for the space creation form, gradient generator popup, and sidebar space swipe gesture.
//!
//! Only transient UI state lives here; persisted spaces and their appearances live in [`zendia_browser_core`].

use freya::prelude::*;
use zendia_browser_core::model::SpaceId;
use zendia_browser_core::space_appearance::SpaceAppearance;

/// Active gesture mode while swiping horizontally over the sidebar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpaceSwipeMode {
    #[default]
    Idle,
    /// Translating between existing spaces.
    SwitchSpace,
    /// Swiping past the last space to reveal the `+` add-space badge.
    AddSpace,
    /// Swiping past the first space toward the Library edge (`SWP-04`).
    OpenLibrary,
}

/// Transient view state for an in-progress horizontal space swipe in the sidebar.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpaceSwipeViewState {
    pub mode: SpaceSwipeMode,
    /// Horizontal strip translation in logical pixels (`< 0` slides toward next space, `> 0` toward previous).
    pub offset_x: f32,
    /// Normalized progress `0.0..=1.25` for the add-space edge gesture (`>= 1.0` means ready to add).
    pub add_progress: f32,
    /// Adjacent space being previewed during a `SwitchSpace` swipe.
    pub target_space_id: Option<SpaceId>,
    /// Monotonic tick used to settle discrete horizontal wheel bursts after a short idle pause.
    pub generation: u64,
    /// When true, freezes the gesture state for gallery previews so no timer clears it.
    pub is_preview: bool,
}

/// View-local state shared across the window shell, sidebar, space creation form, and gradient generator popup.
#[derive(Clone, Copy, PartialEq)]
pub struct SpaceEditingState {
    // Space creation form draft state
    pub create_form_open: State<bool>,
    pub draft_name: State<String>,
    pub draft_icon: State<String>,
    pub draft_profile: State<String>,
    pub draft_appearance: State<SpaceAppearance>,
    pub icon_picker_open: State<bool>,
    pub icon_picker_anchor: State<(f32, f32)>,
    pub profile_menu_open: State<bool>,

    // Gradient generator popup state
    pub theme_picker_open: State<bool>,
    /// Target space being edited; `None` means the active space (unless `theme_picker_for_draft` is true).
    pub theme_picker_space_id: State<Option<SpaceId>>,
    /// When true, gradient generator edits mutate `draft_appearance` instead of an existing space in the core.
    pub theme_picker_for_draft: State<bool>,
    pub theme_picker_anchor: State<(f32, f32)>,
    pub preset_page: State<usize>,

    // Live existing-space icon picker (`<leader><Tab>i` outside Library, `NAV-19`)
    pub live_icon_picker_space: State<Option<SpaceId>>,
    pub live_icon_picker_anchor: State<(f32, f32)>,

    // Non-empty space delete confirmation dialog (`<leader><Tab>d`, `NAV-13`)
    pub delete_confirm_space: State<Option<(SpaceId, String, usize)>>,

    // Sidebar swipe gesture state
    pub swipe: State<SpaceSwipeViewState>,
}

impl SpaceEditingState {
    /// Opens the space creation form in the sidebar with a fresh empty draft.
    pub fn open_create_form(&self) {
        let mut name = self.draft_name;
        name.set(String::new());
        let mut icon = self.draft_icon;
        icon.set(String::new());
        let mut profile = self.draft_profile;
        profile.set("Default".to_string());
        let mut appearance = self.draft_appearance;
        appearance.set(SpaceAppearance::empty());
        let mut icon_picker = self.icon_picker_open;
        icon_picker.set_if_modified(false);
        let mut profile_menu = self.profile_menu_open;
        profile_menu.set_if_modified(false);
        let mut picker_open = self.theme_picker_open;
        picker_open.set_if_modified(false);
        let mut for_draft = self.theme_picker_for_draft;
        for_draft.set_if_modified(false);
        let mut open = self.create_form_open;
        open.set_if_modified(true);
    }

    /// Closes the space creation form and any sub-popups it opened.
    pub fn close_create_form(&self) {
        let mut open = self.create_form_open;
        open.set_if_modified(false);
        let mut icon_picker = self.icon_picker_open;
        icon_picker.set_if_modified(false);
        let mut profile_menu = self.profile_menu_open;
        profile_menu.set_if_modified(false);
        if *self.theme_picker_for_draft.peek() {
            let mut picker_open = self.theme_picker_open;
            picker_open.set_if_modified(false);
            let mut for_draft = self.theme_picker_for_draft;
            for_draft.set_if_modified(false);
        }
    }

    /// Opens the gradient generator popup for an existing space (or the active space when `None`).
    pub fn open_theme_picker(&self, space_id: Option<SpaceId>, anchor: Option<(f32, f32)>) {
        if let Some(pos) = anchor {
            let mut a = self.theme_picker_anchor;
            a.set(pos);
        }
        let mut target = self.theme_picker_space_id;
        target.set(space_id);
        let mut for_draft = self.theme_picker_for_draft;
        for_draft.set_if_modified(false);
        let mut open = self.theme_picker_open;
        open.set_if_modified(true);
    }

    /// Opens the gradient generator popup targeting the in-progress space creation form draft.
    pub fn open_theme_picker_for_draft(&self, anchor: Option<(f32, f32)>) {
        if let Some(pos) = anchor {
            let mut a = self.theme_picker_anchor;
            a.set(pos);
        }
        let mut target = self.theme_picker_space_id;
        target.set(None);
        let mut for_draft = self.theme_picker_for_draft;
        for_draft.set_if_modified(true);
        let mut open = self.theme_picker_open;
        open.set_if_modified(true);
    }

    /// Closes the gradient generator popup.
    pub fn close_theme_picker(&self) {
        let mut open = self.theme_picker_open;
        open.set_if_modified(false);
        let mut for_draft = self.theme_picker_for_draft;
        for_draft.set_if_modified(false);
    }

    /// Opens the searchable icon picker anchored to the sidebar for an existing space (`NAV-19`).
    pub fn open_live_icon_picker(&self, space_id: SpaceId, anchor: Option<(f32, f32)>) {
        if let Some(pos) = anchor {
            let mut a = self.live_icon_picker_anchor;
            a.set(pos);
        }
        let mut space = self.live_icon_picker_space;
        space.set(Some(space_id));
    }

    pub fn close_live_icon_picker(&self) {
        let mut space = self.live_icon_picker_space;
        space.set_if_modified(None);
    }

    /// Opens the confirmation dialog before deleting a non-empty space (`NAV-13`).
    pub fn open_delete_confirm(&self, space_id: SpaceId, space_name: String, tab_count: usize) {
        let mut confirm = self.delete_confirm_space;
        confirm.set(Some((space_id, space_name, tab_count)));
    }

    pub fn close_delete_confirm(&self) {
        let mut confirm = self.delete_confirm_space;
        confirm.set_if_modified(None);
    }

    pub fn has_modal_open(&self) -> bool {
        self.live_icon_picker_space.peek().is_some() || self.delete_confirm_space.peek().is_some()
    }

    /// Whether the space creation form is open and capturing plain text input.
    pub fn captures_text_input(&self) -> bool {
        *self.create_form_open.peek() || self.live_icon_picker_space.peek().is_some()
    }

    /// Returns a live preview appearance when the space creation form is active and has dots or texture set.
    pub fn active_draft_appearance(&self) -> Option<SpaceAppearance> {
        if *self.create_form_open.read() {
            let app = self.draft_appearance.read().clone();
            if !app.dots.is_empty() || app.texture_amount > 0.0 {
                return Some(app);
            }
        }
        None
    }

    /// Resets all transient spaces-editing state (used when switching gallery sections).
    pub fn reset(&self) {
        self.close_create_form();
        self.close_theme_picker();
        self.close_live_icon_picker();
        self.close_delete_confirm();
        let mut page = self.preset_page;
        page.set_if_modified(0);
        let mut swipe = self.swipe;
        swipe.set(SpaceSwipeViewState::default());
    }
}

/// Installs [`SpaceEditingState`] into Freya context for the current window.
pub fn use_provide_space_editing_state() -> SpaceEditingState {
    let create_form_open = use_state(|| false);
    let draft_name = use_state(String::new);
    let draft_icon = use_state(String::new);
    let draft_profile = use_state(|| "Default".to_string());
    let draft_appearance = use_state(SpaceAppearance::empty);
    let icon_picker_open = use_state(|| false);
    let icon_picker_anchor = use_state(|| (194.0f32, 224.0f32));
    let profile_menu_open = use_state(|| false);

    let theme_picker_open = use_state(|| false);
    let theme_picker_space_id = use_state(|| None::<SpaceId>);
    let theme_picker_for_draft = use_state(|| false);
    let theme_picker_anchor = use_state(|| (194.0f32, 86.0f32));
    let preset_page = use_state(|| 0usize);

    let live_icon_picker_space = use_state(|| None::<SpaceId>);
    let live_icon_picker_anchor = use_state(|| (194.0f32, 86.0f32));
    let delete_confirm_space = use_state(|| None::<(SpaceId, String, usize)>);

    let swipe = use_state(SpaceSwipeViewState::default);

    use_provide_context(|| SpaceEditingState {
        create_form_open,
        draft_name,
        draft_icon,
        draft_profile,
        draft_appearance,
        icon_picker_open,
        icon_picker_anchor,
        profile_menu_open,
        theme_picker_open,
        theme_picker_space_id,
        theme_picker_for_draft,
        theme_picker_anchor,
        preset_page,
        live_icon_picker_space,
        live_icon_picker_anchor,
        delete_confirm_space,
        swipe,
    })
}

/// Consumes [`SpaceEditingState`] if installed in this window.
pub fn try_use_space_editing_state() -> Option<SpaceEditingState> {
    try_consume_context::<SpaceEditingState>()
}
