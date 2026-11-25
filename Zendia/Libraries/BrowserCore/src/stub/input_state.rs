/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for vim statusline settings, announcements, counts, and which-key commands.

use super::StubBackend;
use crate::commands::{Command, CommandError, CommandResult};
use crate::events::{Event, ViewRequest};
use crate::input_state::{
    DEFAULT_ANNOUNCEMENT_TIMEOUT_MS, SAMPLE_INPUT_STEP_COUNT, StatuslineAnnouncement, StatuslineShow,
    sample_input_state,
};

impl StubBackend {
    /// Updates the pending vim count prefix published by InputServer.
    pub fn set_pending_count(&self, count: Option<u32>) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.pending_count = count;
        }
        self.bus.publish(Event::PendingCountChanged { count });
    }

    /// Updates the statusline resting visibility setting (`always`, `shrink`, `never`).
    pub fn set_statusline_show(&self, show: StatuslineShow) {
        let settings = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.statusline.show = show;
            state.model.statusline.clone()
        };
        self.bus.publish(Event::StatuslineSettingsChanged { settings });
    }

    /// Publishes a short transient announcement shown beside the vim mode indicator.
    pub fn announce_statusline(&self, message: String, timeout_ms: Option<u64>) {
        let announcement = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let next_id = state.model.statusline_announcement.as_ref().map(|a| a.id).unwrap_or(0) + 1;
            let announcement = StatuslineAnnouncement {
                id: next_id,
                message,
                timeout_ms: timeout_ms.unwrap_or(DEFAULT_ANNOUNCEMENT_TIMEOUT_MS),
            };
            state.model.statusline_announcement = Some(announcement.clone());
            announcement
        };
        self.bus.publish(Event::StatuslineAnnouncementChanged {
            announcement: Some(announcement),
        });
    }

    /// Clears any active statusline announcement.
    pub fn clear_statusline_announcement(&self) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.model.statusline_announcement = None;
        }
        self.bus
            .publish(Event::StatuslineAnnouncementChanged { announcement: None });
    }

    /// Advances the debug input cycle through every vim mode, count, pending key, and which-key state.
    pub(super) fn cycle_sample_input_state_impl(&self) {
        let step = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.debug_input_step = (state.debug_input_step + 1) % SAMPLE_INPUT_STEP_COUNT;
            state.debug_input_step
        };
        let sample = sample_input_state(step);
        self.set_input_mode(sample.mode);
        self.set_pending_count(sample.count);
        self.set_pending_keys(sample.pending_keys);
        self.set_which_key(sample.which_key);
        match sample.announcement {
            Some(message) => self.announce_statusline(message, None),
            None => self.clear_statusline_announcement(),
        }
    }

    /// Handles statusline and which-key commands routed from `StubBackend::invoke`.
    pub(super) fn handle_input_state_command(&self, command: Command) -> CommandResult {
        match command {
            Command::StatuslineSetShow { show } => {
                self.set_statusline_show(show);
                Ok(())
            }
            Command::StatuslineAnnounce { message, timeout_ms } => {
                self.announce_statusline(message, timeout_ms);
                Ok(())
            }
            Command::StatuslineClearAnnouncement => {
                self.clear_statusline_announcement();
                Ok(())
            }
            Command::WhichKeyShowAll => {
                let mode = self
                    .state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .model
                    .input_mode;
                self.request_view(ViewRequest::WhichKeyAll { mode });
                Ok(())
            }
            Command::WhichKeyHide => {
                self.set_pending_count(None);
                self.set_pending_keys(Vec::new());
                self.set_which_key(None);
                Ok(())
            }
            other => Err(CommandError::UnknownCommand(other.name().to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BrowserCore;
    use crate::model::InputMode;

    #[test]
    fn statusline_settings_announcements_and_which_key_commands_round_trip() {
        let core = StubBackend::new();
        let mut view = core.model();
        let sub = core.subscribe();

        core.invoke(Command::StatuslineSetShow {
            show: StatuslineShow::Always,
        })
        .unwrap();
        core.invoke(Command::StatuslineAnnounce {
            message: "Copied URL".to_string(),
            timeout_ms: Some(1500),
        })
        .unwrap();
        core.invoke(Command::WhichKeyShowAll).unwrap();

        let mut saw_all_requested = false;
        while let Ok(event) = sub.try_recv() {
            if matches!(
                event,
                Event::ViewRequested {
                    request: ViewRequest::WhichKeyAll {
                        mode: InputMode::Normal
                    },
                    ..
                }
            ) {
                saw_all_requested = true;
            }
            view.apply_event(&event);
        }

        assert!(saw_all_requested);
        assert_eq!(view.statusline.show, StatuslineShow::Always);
        assert_eq!(
            view.statusline_announcement.as_ref().map(|a| a.message.as_str()),
            Some("Copied URL")
        );
        assert_eq!(view, core.model());

        core.invoke(Command::StatuslineClearAnnouncement).unwrap();
        while let Ok(event) = sub.try_recv() {
            view.apply_event(&event);
        }
        assert!(view.statusline_announcement.is_none());
        assert_eq!(view, core.model());
    }
}
