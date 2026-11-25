/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub backend command handling for Phase 10 onboarding, Shield, and browser import commands.

use super::StubBackend;
use crate::BrowserCore;
use crate::commands::{Command, CommandError, CommandResult};
use crate::events::{Event, ViewRequest};
use crate::model::{AgentId, TabKind};
use crate::onboarding::{
    OnboardingPage, OnboardingProfileRecord, OnboardingState, find_essential_preset, resolve_search_engine_by_name,
};
use crate::tab_source::TabUpdate;

impl StubBackend {
    /// Loads a persisted onboarding profile snapshot (used by the real window on startup before any UI opens).
    pub fn apply_onboarding_profile(&self, record: Option<OnboardingProfileRecord>) {
        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            match record {
                Some(rec) => {
                    state.model.onboarding.apply_profile_record(&rec);
                    state.model.vim_mode_enabled = rec.vim_enabled;
                    state.model.shield_enabled = rec.shield_enabled;
                    if let Some(engine) = resolve_search_engine_by_name(&rec.search_engine) {
                        state.model.search_engine = engine;
                    }
                    if let Some(agent_id) = &rec.agent_id
                        && state
                            .model
                            .agents
                            .iter()
                            .any(|agent| agent.available && agent.id.0 == *agent_id)
                    {
                        state.model.default_agent_id = Some(AgentId::new(agent_id.clone()));
                    }
                }
                None => {
                    state.model.onboarding = OnboardingState::first_run();
                }
            }
            state.model.onboarding.clone()
        };
        self.bus.publish(Event::OnboardingChanged { state: onboarding });
    }

    pub(super) fn handle_onboarding_command(&self, command: Command) -> CommandResult {
        match command {
            Command::OnboardingOpen => self.handle_onboarding_open(),
            Command::OnboardingRestart => self.handle_onboarding_restart(),
            Command::OnboardingGoto { page } => self.handle_onboarding_goto(page),
            Command::OnboardingNext => self.handle_onboarding_next(),
            Command::OnboardingBack => self.handle_onboarding_back(),
            Command::OnboardingFinish => self.handle_onboarding_finish(),
            Command::OnboardingSkip => self.handle_onboarding_skip(),
            Command::OnboardingSkipApps => self.handle_onboarding_skip_apps(),
            Command::OnboardingSetVim { enabled } => self.handle_onboarding_set_vim(enabled),
            Command::OnboardingSetAgent { agent_id } => self.handle_onboarding_set_agent(agent_id),
            Command::OnboardingSetShield { enabled } => self.handle_onboarding_set_shield(enabled),
            Command::OnboardingSetImport { source_key, profile_id } => {
                self.handle_onboarding_set_import(source_key, profile_id)
            }
            Command::OnboardingSetSearchEngine { name } => self.handle_onboarding_set_search_engine(name),
            Command::OnboardingSetDefaultBrowser { enabled } => self.handle_onboarding_set_default_browser(enabled),
            Command::OnboardingToggleEssential { url } => self.handle_onboarding_toggle_essential(url),
            Command::ShieldSetEnabled { enabled } => self.handle_shield_set_enabled(enabled),
            Command::ImportStart { source_key, profile_id } => self.handle_import_start(source_key, profile_id),
            Command::ImportSetDefaultBrowser { enabled } => self.handle_import_set_default_browser(enabled),
            _ => unreachable!("non-onboarding command routed to handle_onboarding_command"),
        }
    }

    fn publish_onboarding(&self, state: OnboardingState) {
        self.bus.publish(Event::OnboardingChanged { state });
    }

    fn handle_onboarding_open(&self) -> CommandResult {
        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.onboarding.active = true;
            state.model.onboarding.clone()
        };
        self.publish_onboarding(onboarding);
        self.request_view(ViewRequest::OnboardingOpen);
        Ok(())
    }

    fn handle_onboarding_restart(&self) -> CommandResult {
        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let mut next = OnboardingState::first_run();
            next.choices.vim_enabled = state.model.vim_mode_enabled;
            next.choices.shield_enabled = state.model.shield_enabled;
            next.choices.search_engine = state.model.search_engine.name.clone();
            if let Some(default_agent) = &state.model.default_agent_id {
                next.choices.agent_id = Some(default_agent.0.clone());
            }
            next.import_sources = state.model.onboarding.import_sources.clone();
            next.agent_catalog = state.model.onboarding.agent_catalog.clone();
            state.model.onboarding = next;
            state.model.onboarding.clone()
        };
        self.publish_onboarding(onboarding);
        self.request_view(ViewRequest::OnboardingOpen);
        Ok(())
    }

    fn handle_onboarding_goto(&self, page: OnboardingPage) -> CommandResult {
        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.onboarding.active = true;
            state.model.onboarding.navigate_to(page);
            state.model.onboarding.clone()
        };
        self.publish_onboarding(onboarding);
        Ok(())
    }

    fn commit_page_choices(&self, page: OnboardingPage) -> CommandResult {
        let choices = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.onboarding.choices.clone()
        };
        match page {
            OnboardingPage::Start | OnboardingPage::Apps | OnboardingPage::Go => Ok(()),
            OnboardingPage::Keys => self.invoke(Command::VimSetEnabled {
                enabled: choices.vim_enabled,
            }),
            OnboardingPage::Agents => {
                if let Some(agent_id) = choices.agent_id {
                    self.apply_agent_choice(&agent_id)?;
                }
                Ok(())
            }
            OnboardingPage::Shield => self.invoke(Command::ShieldSetEnabled {
                enabled: choices.shield_enabled,
            }),
            OnboardingPage::MoveIn => {
                if let Some(source_key) = choices.import_source_key {
                    self.invoke(Command::ImportStart {
                        source_key,
                        profile_id: choices.import_profile_id,
                    })?;
                }
                if let Some(engine) = resolve_search_engine_by_name(&choices.search_engine) {
                    self.invoke(Command::SettingsSetSearchEngine {
                        name: engine.name,
                        search_url: engine.search_url,
                    })?;
                }
                self.invoke(Command::ImportSetDefaultBrowser {
                    enabled: choices.default_browser,
                })
            }
        }
    }

    fn apply_agent_choice(&self, agent_id: &str) -> CommandResult {
        let is_core_agent = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state
                .model
                .agents
                .iter()
                .any(|agent| agent.available && agent.id.0 == agent_id)
        };
        if is_core_agent {
            self.invoke(Command::AgentSelect {
                agent_id: Some(AgentId::new(agent_id)),
            })
        } else {
            eprintln!("zendia: stub onboarding agent selection `{agent_id}` — AgentServer CLI connector not wired yet");
            Ok(())
        }
    }

    fn apply_essential_choices(&self) -> CommandResult {
        let urls = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.onboarding.choices.essential_urls.clone()
        };
        for url in urls {
            let existing_tab = {
                let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                let normalized = url.trim().trim_end_matches('/');
                state.model.tabs.iter().find_map(|tab| {
                    tab.url
                        .trim()
                        .trim_end_matches('/')
                        .eq_ignore_ascii_case(normalized)
                        .then_some((tab.id, tab.kind))
                })
            };

            if let Some((tab_id, kind)) = existing_tab {
                if kind != TabKind::Essential {
                    let _ = self.invoke(Command::TabAddToEssentials { tab_id: Some(tab_id) });
                }
                continue;
            }

            self.invoke(Command::TabNew {
                url: Some(url.clone()),
                after: None,
                activate: false,
            })?;
            let new_tab_id = {
                let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                state.last_opened_tab
            };
            if let Some(tab_id) = new_tab_id {
                if let Some(preset) = find_essential_preset(&url) {
                    {
                        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                        if let Some(tab) = state.model.tabs.iter_mut().find(|t| t.id == tab_id) {
                            tab.favicon = Some(preset.icon.to_string());
                        }
                    }
                    self.feed_tab_updated(
                        tab_id,
                        TabUpdate {
                            title: Some(preset.name.to_string()),
                            ..TabUpdate::default()
                        },
                    );
                }
                let _ = self.invoke(Command::TabAddToEssentials { tab_id: Some(tab_id) });
            }
        }
        Ok(())
    }

    fn handle_onboarding_next(&self) -> CommandResult {
        let current = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.onboarding.page
        };
        if current == OnboardingPage::Go {
            return self.handle_onboarding_finish();
        }
        self.commit_page_choices(current)?;
        if let Some(next_page) = current.next() {
            let onboarding = {
                let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                state.model.onboarding.active = true;
                state.model.onboarding.navigate_to(next_page);
                state.model.onboarding.clone()
            };
            self.publish_onboarding(onboarding);
        }
        Ok(())
    }

    fn handle_onboarding_back(&self) -> CommandResult {
        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(prev) = state.model.onboarding.page.prev_chapter() else {
                return Ok(());
            };
            state.model.onboarding.navigate_to(prev);
            state.model.onboarding.clone()
        };
        self.publish_onboarding(onboarding);
        Ok(())
    }

    fn handle_onboarding_skip_apps(&self) -> CommandResult {
        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.onboarding.choices.essential_urls.clear();
            state.model.onboarding.navigate_to(OnboardingPage::Go);
            state.model.onboarding.clone()
        };
        self.publish_onboarding(onboarding);
        Ok(())
    }

    fn handle_onboarding_finish(&self) -> CommandResult {
        let choices = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.onboarding.choices.clone()
        };

        // Ensure every choice is applied through its real browser core command even if the user jumped directly to Go.
        self.invoke(Command::VimSetEnabled {
            enabled: choices.vim_enabled,
        })?;
        if let Some(agent_id) = &choices.agent_id {
            self.apply_agent_choice(agent_id)?;
        }
        self.invoke(Command::ShieldSetEnabled {
            enabled: choices.shield_enabled,
        })?;
        if let Some(engine) = resolve_search_engine_by_name(&choices.search_engine) {
            self.invoke(Command::SettingsSetSearchEngine {
                name: engine.name,
                search_url: engine.search_url,
            })?;
        }
        if let Some(source_key) = &choices.import_source_key {
            let already_imported = {
                let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                state.model.onboarding.last_import.as_ref().map(|(k, _)| k.as_str()) == Some(source_key.as_str())
            };
            if !already_imported {
                self.invoke(Command::ImportStart {
                    source_key: source_key.clone(),
                    profile_id: choices.import_profile_id.clone(),
                })?;
            }
        }
        if choices.default_browser {
            let already_applied = {
                let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                state.model.onboarding.default_browser_applied
            };
            if !already_applied {
                self.invoke(Command::ImportSetDefaultBrowser { enabled: true })?;
            }
        }
        self.apply_essential_choices()?;

        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.onboarding.seen = true;
            state.model.onboarding.active = false;
            state.model.onboarding.page = OnboardingPage::Go;
            state.model.onboarding.reached = OnboardingPage::Go;
            state.model.onboarding.clone()
        };
        self.publish_onboarding(onboarding);
        self.request_view(ViewRequest::OnboardingClose);
        Ok(())
    }

    fn handle_onboarding_skip(&self) -> CommandResult {
        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.onboarding.seen = true;
            state.model.onboarding.active = false;
            state.model.onboarding.clone()
        };
        self.publish_onboarding(onboarding);
        self.request_view(ViewRequest::OnboardingClose);
        Ok(())
    }

    fn handle_onboarding_set_vim(&self, enabled: bool) -> CommandResult {
        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.onboarding.choices.vim_enabled = enabled;
            state.model.onboarding.clone()
        };
        self.publish_onboarding(onboarding);
        Ok(())
    }

    fn handle_onboarding_set_agent(&self, agent_id: String) -> CommandResult {
        let trimmed = agent_id.trim().to_string();
        if trimmed.is_empty() {
            return Err(CommandError::InvalidArgument {
                command: "onboarding.set_agent".to_string(),
                message: "agent_id must not be empty".to_string(),
            });
        }
        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.onboarding.choices.agent_id = Some(trimmed);
            state.model.onboarding.clone()
        };
        self.publish_onboarding(onboarding);
        Ok(())
    }

    fn handle_onboarding_set_shield(&self, enabled: bool) -> CommandResult {
        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.onboarding.choices.shield_enabled = enabled;
            state.model.onboarding.clone()
        };
        self.publish_onboarding(onboarding);
        Ok(())
    }

    fn handle_onboarding_set_import(&self, source_key: Option<String>, profile_id: Option<String>) -> CommandResult {
        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let normalized_key = source_key.filter(|k| !k.trim().is_empty());
            let resolved_profile = if let Some(key) = &normalized_key {
                profile_id.filter(|p| !p.trim().is_empty()).or_else(|| {
                    state
                        .model
                        .onboarding
                        .import_sources
                        .iter()
                        .find(|source| source.key == *key)
                        .and_then(|source| source.profiles.first().map(|p| p.id.clone()))
                })
            } else {
                None
            };
            state.model.onboarding.choices.import_source_key = normalized_key;
            state.model.onboarding.choices.import_profile_id = resolved_profile;
            state.model.onboarding.clone()
        };
        self.publish_onboarding(onboarding);
        Ok(())
    }

    fn handle_onboarding_set_search_engine(&self, engine_name: String) -> CommandResult {
        let Some(engine) = resolve_search_engine_by_name(&engine_name) else {
            return Err(CommandError::InvalidArgument {
                command: "onboarding.set_search_engine".to_string(),
                message: format!("unknown search engine `{engine_name}`"),
            });
        };
        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.onboarding.choices.search_engine = engine.name.clone();
            state.model.onboarding.clone()
        };
        self.publish_onboarding(onboarding);
        self.invoke(Command::SettingsSetSearchEngine {
            name: engine.name,
            search_url: engine.search_url,
        })
    }

    fn handle_onboarding_set_default_browser(&self, enabled: bool) -> CommandResult {
        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.onboarding.choices.default_browser = enabled;
            state.model.onboarding.clone()
        };
        self.publish_onboarding(onboarding);
        Ok(())
    }

    fn handle_onboarding_toggle_essential(&self, url_or_id: String) -> CommandResult {
        let resolved_url = find_essential_preset(&url_or_id)
            .map(|preset| preset.url.to_string())
            .unwrap_or_else(|| url_or_id.trim().to_string());
        if resolved_url.is_empty() {
            return Err(CommandError::InvalidArgument {
                command: "onboarding.toggle_essential".to_string(),
                message: "url must not be empty".to_string(),
            });
        }
        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let urls = &mut state.model.onboarding.choices.essential_urls;
            if let Some(pos) = urls.iter().position(|u| u.eq_ignore_ascii_case(&resolved_url)) {
                urls.remove(pos);
            } else {
                urls.push(resolved_url);
            }
            state.model.onboarding.clone()
        };
        self.publish_onboarding(onboarding);
        Ok(())
    }

    fn handle_shield_set_enabled(&self, enabled: bool) -> CommandResult {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.shield_enabled = enabled;
            state.model.onboarding.choices.shield_enabled = enabled;
        }
        eprintln!("zendia: stub shield.set_enabled enabled={enabled} — uBlock Origin engine hook not connected yet");
        self.bus.publish(Event::ShieldChanged { enabled });
        Ok(())
    }

    fn handle_import_start(&self, source_key: String, profile_id: Option<String>) -> CommandResult {
        let key = source_key.trim().to_string();
        if key.is_empty() {
            return Err(CommandError::InvalidArgument {
                command: "import.start".to_string(),
                message: "source_key must not be empty".to_string(),
            });
        }
        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.onboarding.last_import = Some((key.clone(), profile_id.clone()));
            state.model.onboarding.clone()
        };
        eprintln!(
            "zendia: stub import.start source=`{key}` profile={profile_id:?} — browser migrator not connected yet"
        );
        self.bus.publish(Event::ImportStarted {
            source_key: key,
            profile_id,
        });
        self.publish_onboarding(onboarding);
        Ok(())
    }

    fn handle_import_set_default_browser(&self, enabled: bool) -> CommandResult {
        let onboarding = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.onboarding.default_browser_applied = enabled;
            state.model.onboarding.clone()
        };
        eprintln!(
            "zendia: stub import.set_default_browser enabled={enabled} — OS default browser integration not connected yet"
        );
        self.publish_onboarding(onboarding);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Event;
    use crate::model::WindowId;

    #[test]
    fn onboarding_flow_applies_all_choices_via_commands_and_syncs_view_model() {
        let core = StubBackend::new();
        let mut view_model = core.model();
        let sub = core.subscribe();

        // Start onboarding via restart command.
        core.invoke_in(WindowId(1), Command::OnboardingRestart).unwrap();
        let mut saw_open = false;
        while let Ok(event) = sub.try_recv() {
            if matches!(
                event,
                Event::ViewRequested {
                    window: Some(WindowId(1)),
                    request: ViewRequest::OnboardingOpen,
                }
            ) {
                saw_open = true;
            }
            view_model.apply_event(&event);
        }
        assert!(saw_open);
        assert_eq!(view_model, core.model());
        assert!(!view_model.onboarding.seen);
        assert!(view_model.onboarding.active);
        assert_eq!(view_model.onboarding.page, OnboardingPage::Start);

        // Start -> Keys -> enable vim -> Agents
        core.invoke(Command::OnboardingNext).unwrap();
        assert_eq!(core.model().onboarding.page, OnboardingPage::Keys);
        core.invoke(Command::OnboardingSetVim { enabled: true }).unwrap();
        core.invoke(Command::OnboardingNext).unwrap();
        assert!(core.model().vim_mode_enabled);
        assert_eq!(core.model().onboarding.page, OnboardingPage::Agents);

        // Select Codex -> Shield
        core.invoke(Command::OnboardingSetAgent {
            agent_id: "codex".to_string(),
        })
        .unwrap();
        core.invoke(Command::OnboardingNext).unwrap();
        assert_eq!(core.model().default_agent_id, Some(AgentId::new("codex")));
        assert_eq!(core.model().onboarding.page, OnboardingPage::Shield);

        // Toggle Shield off -> Move in
        core.invoke(Command::OnboardingSetShield { enabled: false }).unwrap();
        core.invoke(Command::OnboardingNext).unwrap();
        assert!(!core.model().shield_enabled);
        assert_eq!(core.model().onboarding.page, OnboardingPage::MoveIn);

        // Pick Chrome import, DuckDuckGo search, and default browser -> Apps
        core.invoke(Command::OnboardingSetImport {
            source_key: Some("chrome".to_string()),
            profile_id: Some("Default".to_string()),
        })
        .unwrap();
        core.invoke(Command::OnboardingSetSearchEngine {
            name: "DuckDuckGo".to_string(),
        })
        .unwrap();
        assert_eq!(core.model().search_engine.name, "DuckDuckGo");
        core.invoke(Command::OnboardingSetDefaultBrowser { enabled: true })
            .unwrap();
        core.invoke(Command::OnboardingNext).unwrap();
        assert_eq!(
            core.model().onboarding.last_import,
            Some(("chrome".to_string(), Some("Default".to_string())))
        );
        assert!(core.model().onboarding.default_browser_applied);
        assert_eq!(core.model().onboarding.page, OnboardingPage::Apps);

        // Toggle GitHub & Discord essentials -> Go -> Finish
        let essentials_before = core.model().essential_tabs().len();
        core.invoke(Command::OnboardingToggleEssential {
            url: "https://github.com".to_string(),
        })
        .unwrap();
        core.invoke(Command::OnboardingToggleEssential {
            url: "https://discord.com".to_string(),
        })
        .unwrap();
        core.invoke(Command::OnboardingNext).unwrap();
        assert_eq!(core.model().onboarding.page, OnboardingPage::Go);

        core.invoke_in(WindowId(1), Command::OnboardingFinish).unwrap();
        let mut saw_close = false;
        while let Ok(event) = sub.try_recv() {
            if matches!(
                event,
                Event::ViewRequested {
                    window: Some(WindowId(1)),
                    request: ViewRequest::OnboardingClose,
                }
            ) {
                saw_close = true;
            }
            view_model.apply_event(&event);
        }
        assert!(saw_close);
        assert_eq!(view_model, core.model());
        assert!(view_model.onboarding.seen);
        assert!(!view_model.onboarding.active);
        assert_eq!(view_model.essential_tabs().len(), essentials_before + 2);
        assert!(
            view_model
                .essential_tabs()
                .iter()
                .any(|tab| tab.url == "https://github.com")
        );
        assert!(
            view_model
                .essential_tabs()
                .iter()
                .any(|tab| tab.url == "https://discord.com")
        );
    }

    #[test]
    fn onboarding_skip_marks_seen_and_closes_without_enabling_vim() {
        let core = StubBackend::new();
        core.invoke(Command::OnboardingRestart).unwrap();
        assert!(!core.model().onboarding.seen);
        core.invoke(Command::OnboardingSkip).unwrap();
        assert!(core.model().onboarding.seen);
        assert!(!core.model().onboarding.active);
    }

    #[test]
    fn onboarding_goto_and_back_respect_chapter_bounds() {
        let core = StubBackend::new();
        core.invoke(Command::OnboardingRestart).unwrap();
        core.invoke(Command::OnboardingGoto {
            page: OnboardingPage::Shield,
        })
        .unwrap();
        assert_eq!(core.model().onboarding.page, OnboardingPage::Shield);
        assert_eq!(core.model().onboarding.reached, OnboardingPage::Shield);

        core.invoke(Command::OnboardingBack).unwrap();
        assert_eq!(core.model().onboarding.page, OnboardingPage::Agents);
        assert_eq!(core.model().onboarding.direction, -1);

        core.invoke(Command::OnboardingBack).unwrap();
        assert_eq!(core.model().onboarding.page, OnboardingPage::Keys);

        // Back on chapter 01 (Keys) stays on Keys.
        core.invoke(Command::OnboardingBack).unwrap();
        assert_eq!(core.model().onboarding.page, OnboardingPage::Keys);
    }
}
