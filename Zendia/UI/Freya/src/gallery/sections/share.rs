/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 12 gallery section: Share link confirmation modal dialog over the browser window shell.

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::core_view::use_core;
use crate::ui::page::{PageArea, PaneMode};
use crate::ui::share::ShareConfirmDialog;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, Rgba, ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::model::{BrowserModel, Space, SpaceGradient, SpaceId, Tab, TabId, TabKind};
use zendia_browser_core::share::build_share_payload;

/// Gallery section showcasing Phase 12 Share link confirmation states.
#[derive(Clone)]
pub struct ShareSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
    key: DiffKey,
}

impl PartialEq for ShareSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

impl KeyExt for ShareSection {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ShareSection {
    pub fn new(state_id: impl Into<String>, gallery_core: Arc<GalleryCore>) -> Self {
        Self {
            state_id: state_id.into(),
            gallery_core,
            key: DiffKey::None,
        }
    }
}

fn make_tab(id: u64, space_id: SpaceId, title: &str, url: &str, favicon: &str) -> Tab {
    Tab {
        id: TabId(id),
        view_id: None,
        url: url.to_string(),
        title: title.to_string(),
        custom_title: None,
        favicon: Some(favicon.to_string()),
        is_loading: false,
        can_navigate_back: false,
        can_navigate_forward: false,
        is_audible: false,
        is_muted: false,
        kind: TabKind::Normal,
        space_id: Some(space_id),
        folder_id: None,
        split_group_id: None,
        closing: false,
    }
}

fn build_fixture_model() -> BrowserModel {
    let mut model = super::gallery_sample_model();
    let space1 = SpaceId(1);
    model.essentials.clear();
    model.pinned_tabs.clear();
    model.folders.clear();
    model.split_groups.clear();
    model.sidebar_notifications.clear();
    model.media_sessions.clear();
    model.show_essentials_promo = false;
    model.spaces = vec![Space {
        id: space1,
        name: "Space".to_string(),
        icon: String::new(),
        gradient: SpaceGradient {
            stops: vec![(102, 76, 98), (118, 86, 114)],
            angle_deg: 135.0,
            opacity: 0.72,
        },
        last_active_tab: Some(TabId(301)),
    }];
    model.tabs = vec![
        make_tab(301, space1, "Repository — Zendia", "https://zendia.local/code", "code"),
        make_tab(
            302,
            space1,
            "Releases — Zendia",
            "https://zendia.local/releases",
            "rocket",
        ),
        make_tab(
            303,
            space1,
            "Design Tokens — Zendia",
            "https://zendia.local/design",
            "flame",
        ),
        make_tab(
            304,
            space1,
            "Architecture — Zendia",
            "https://zendia.local/architecture",
            "code",
        ),
        make_tab(305, space1, "Zendia Documentation", "https://zendia.local/docs", "book"),
    ];
    model.active_space = Some(space1);
    model.active_tab = Some(TabId(301));
    if let Some(win) = model.windows.first_mut() {
        win.active_space = Some(space1);
        win.active_tab = Some(TabId(301));
    }
    model
}

impl Component for ShareSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let location = use_state(|| "https://zendia.local/code".to_string());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut applied_state = use_state(String::new);
        let mut name_draft = use_state(|| "Alex".to_string());
        let mut dont_ask = use_state(|| false);

        if *applied_state.peek() != self.state_id {
            let current_theme = core.model().peek().theme.clone();
            let mut model = build_fixture_model();
            model.theme = current_theme;
            self.gallery_core.stub().replace_model(model);
            match self.state_id.as_str() {
                "confirm-named" => {
                    name_draft.set("Alex".to_string());
                    dont_ask.set(false);
                }
                "confirm-dont-ask" => {
                    name_draft.set("Alex".to_string());
                    dont_ask.set(true);
                }
                _ => {
                    name_draft.set(String::new());
                    dont_ask.set(false);
                }
            }
            applied_state.set(self.state_id.clone());
        }

        let payload = {
            let model = core.model().read();
            build_share_payload(&model, None, None, None)
        };

        let scrim_bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(0, 0, 0, 0.34).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(0, 0, 0, 0.14).to_color(),
        };

        let stage_page = PageArea::new(PaneMode::Placeholder);
        let shell = WindowShell::new(location, location_a11y_id, stage_page)
            .interactive_window_controls(false)
            .key(format!("share-shell-{}", self.state_id));

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .padding(Gaps::new_all(12.0))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .corner_radius(CornerRadius::new_all(10.0))
                    .overflow(Overflow::Clip)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.color(ThemeRole::BorderSubtle)),
                    )
                    .child(shell)
                    .child(
                        rect()
                            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                            .position(Position::new_absolute().left(0.0).top(0.0))
                            .width(Size::percent(100.0))
                            .height(Size::percent(100.0))
                            .background(scrim_bg)
                            .center()
                            .child(ShareConfirmDialog::new(payload, name_draft, dont_ask).key(self.state_id.clone())),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
