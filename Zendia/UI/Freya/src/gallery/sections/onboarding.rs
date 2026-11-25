/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 10 gallery section: all seven onboarding pages, mid-transition chapter/mascot motion, and altered choices
//! for every page, interactive in both light and dark themes.

use crate::gallery::registry::GalleryCore;
use crate::ui::onboarding::{OnboardingView, use_onboarding_state};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::BrowserCore;
use zendia_browser_core::model::{BrowserModel, InputMode};
use zendia_browser_core::onboarding::{
    MoveInFocus, OnboardingPage, OnboardingState, sample_agent_catalog, sample_import_sources,
    sample_multi_profile_import_sources,
};

#[derive(Clone)]
pub struct OnboardingSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
    key: DiffKey,
}

impl PartialEq for OnboardingSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

impl KeyExt for OnboardingSection {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl OnboardingSection {
    pub fn new(state_id: impl Into<String>, gallery_core: Arc<GalleryCore>) -> Self {
        Self {
            state_id: state_id.into(),
            gallery_core,
            key: DiffKey::None,
        }
    }
}

fn build_onboarding_fixture(state_id: &str) -> BrowserModel {
    let mut model = super::gallery_sample_model();
    let mut onb = OnboardingState::first_run();
    onb.agent_catalog = sample_agent_catalog();
    onb.import_sources = sample_import_sources();
    onb.choices.vim_enabled = false;
    onb.choices.shield_enabled = true;
    onb.choices.agent_id = Some("claude-code".to_string());
    onb.choices.search_engine = "Google".to_string();
    onb.choices.default_browser = false;
    onb.choices.essential_urls.clear();

    match state_id {
        "start" | "start-explorer" => {
            onb.page = OnboardingPage::Start;
            onb.reached = OnboardingPage::Start;
        }
        "keys" => {
            onb.page = OnboardingPage::Keys;
            onb.reached = OnboardingPage::Keys;
            onb.choices.vim_enabled = false;
        }
        "keys-vim-on" => {
            onb.page = OnboardingPage::Keys;
            onb.reached = OnboardingPage::Keys;
            onb.choices.vim_enabled = true;
        }
        "agents" => {
            onb.page = OnboardingPage::Agents;
            onb.reached = OnboardingPage::Agents;
            onb.choices.agent_id = Some("claude-code".to_string());
        }
        "agents-codex" => {
            onb.page = OnboardingPage::Agents;
            onb.reached = OnboardingPage::Agents;
            onb.choices.vim_enabled = true;
            onb.choices.agent_id = Some("codex".to_string());
        }
        "shield-off" => {
            onb.page = OnboardingPage::Shield;
            onb.reached = OnboardingPage::Shield;
            onb.choices.shield_enabled = false;
        }
        "shield-on" => {
            onb.page = OnboardingPage::Shield;
            onb.reached = OnboardingPage::Shield;
            onb.choices.vim_enabled = true;
            onb.choices.shield_enabled = true;
        }
        "move-in" => {
            onb.page = OnboardingPage::MoveIn;
            onb.reached = OnboardingPage::MoveIn;
        }
        "move-in-import" => {
            onb.page = OnboardingPage::MoveIn;
            onb.reached = OnboardingPage::MoveIn;
            onb.import_sources = sample_multi_profile_import_sources();
            onb.choices.import_source_key = Some("chrome".to_string());
            onb.choices.import_profile_id = Some("Profile 1".to_string());
        }
        "move-in-search" => {
            onb.page = OnboardingPage::MoveIn;
            onb.reached = OnboardingPage::MoveIn;
            onb.choices.search_engine = "DuckDuckGo".to_string();
        }
        "move-in-default" => {
            onb.page = OnboardingPage::MoveIn;
            onb.reached = OnboardingPage::MoveIn;
            onb.choices.default_browser = true;
        }
        "apps" => {
            onb.page = OnboardingPage::Apps;
            onb.reached = OnboardingPage::Apps;
        }
        "apps-selected" => {
            onb.page = OnboardingPage::Apps;
            onb.reached = OnboardingPage::Apps;
            onb.choices.vim_enabled = true;
            onb.choices.essential_urls = vec![
                "https://obsidian.md".to_string(),
                "https://discord.com".to_string(),
                "https://github.com".to_string(),
                "https://figma.com".to_string(),
            ];
        }
        "go" => {
            onb.page = OnboardingPage::Go;
            onb.reached = OnboardingPage::Go;
            onb.choices.vim_enabled = false;
            onb.choices.shield_enabled = false;
            onb.choices.agent_id = Some("claude-code".to_string());
        }
        "go-configured" => {
            onb.page = OnboardingPage::Go;
            onb.reached = OnboardingPage::Go;
            onb.choices.vim_enabled = true;
            onb.choices.shield_enabled = true;
            onb.choices.agent_id = Some("codex".to_string());
            onb.choices.essential_urls = vec![
                "https://obsidian.md".to_string(),
                "https://discord.com".to_string(),
                "https://github.com".to_string(),
                "https://figma.com".to_string(),
            ];
        }
        "transition" => {
            onb.page = OnboardingPage::Agents;
            onb.reached = OnboardingPage::Agents;
            onb.direction = 1;
            onb.choices.vim_enabled = true;
            onb.choices.agent_id = Some("claude-code".to_string());
        }
        _ => {
            onb.page = OnboardingPage::Start;
            onb.reached = OnboardingPage::Start;
        }
    }

    model.onboarding = onb;
    model
}

#[derive(Default)]
struct FixtureOverrides {
    move_focus: Option<MoveInFocus>,
    start_pose: Option<InputMode>,
    pinned_transition: Option<f32>,
    pinned_morph: Option<(InputMode, f32)>,
    frozen_at: Option<f32>,
}

fn fixture_view_overrides(state_id: &str) -> FixtureOverrides {
    match state_id {
        "start-explorer" => FixtureOverrides {
            start_pose: Some(InputMode::Explorer),
            ..FixtureOverrides::default()
        },
        "keys-vim-on" => FixtureOverrides {
            frozen_at: Some(8.4),
            ..FixtureOverrides::default()
        },
        "shield-off" => FixtureOverrides {
            frozen_at: Some(2.6),
            ..FixtureOverrides::default()
        },
        "shield-on" => FixtureOverrides {
            frozen_at: Some(3.05),
            ..FixtureOverrides::default()
        },
        "move-in-import" => FixtureOverrides {
            move_focus: Some(MoveInFocus::Import),
            frozen_at: Some(2.0),
            ..FixtureOverrides::default()
        },
        "move-in-search" => FixtureOverrides {
            move_focus: Some(MoveInFocus::Search),
            frozen_at: Some(2.3),
            ..FixtureOverrides::default()
        },
        "move-in-default" => FixtureOverrides {
            move_focus: Some(MoveInFocus::DefaultBrowser),
            frozen_at: Some(1.7),
            ..FixtureOverrides::default()
        },
        "transition" => FixtureOverrides {
            pinned_transition: Some(0.45),
            pinned_morph: Some((InputMode::Normal, 0.42)),
            frozen_at: Some(1.6),
            ..FixtureOverrides::default()
        },
        _ => FixtureOverrides::default(),
    }
}

impl Component for OnboardingSection {
    fn render(&self) -> impl IntoElement {
        let onboarding_state = use_onboarding_state();
        let state_id = self.state_id.clone();
        let gallery_core = Arc::clone(&self.gallery_core);

        let mut applied_state = use_state(|| None::<String>);
        if applied_state.peek().as_deref() != Some(state_id.as_str()) {
            let current_theme = gallery_core.model().theme.clone();
            let mut fixture = build_onboarding_fixture(&state_id);
            fixture.theme = current_theme;
            gallery_core.stub().replace_model(fixture);
            let overrides = fixture_view_overrides(&state_id);
            let mut mf = onboarding_state.move_focus;
            mf.set_if_modified(overrides.move_focus.unwrap_or(MoveInFocus::Import));
            let mut sp = onboarding_state.start_pose;
            sp.set_if_modified(overrides.start_pose.unwrap_or(InputMode::Normal));
            let mut paused = onboarding_state.paused;
            paused.set_if_modified(false);
            applied_state.set(Some(state_id.clone()));
        }

        let overrides = fixture_view_overrides(&state_id);

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .overflow(Overflow::Clip)
            .child(
                OnboardingView::new()
                    .still(true)
                    .frozen_at(overrides.frozen_at)
                    .pinned_transition(overrides.pinned_transition)
                    .pinned_mascot_morph(overrides.pinned_morph)
                    .key(state_id),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
