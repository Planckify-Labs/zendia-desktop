/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Media playing in tabs, for the media controls at the foot of the sidebar.
//!
//! The Hub sends no media or audio messages yet, so media sessions come from the stub's sample data and from
//! `debug.sample_media`. When the engine reports media, it feeds the same [`MediaSession`] model.

use crate::model::{BrowserModel, MediaSession, MediaSessionId, TabId};

/// What a media control button does to a session.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MediaControl {
    TogglePlay,
    ToggleMute,
    Previous,
    Next,
    PictureInPicture,
    /// Activates the tab the media plays in.
    Focus,
    /// Hides the controls for this session until it plays again.
    Dismiss,
    /// Moves playback to a position in seconds.
    Seek(f32),
}

impl MediaControl {
    pub fn command_name(self) -> &'static str {
        match self {
            Self::TogglePlay => "media.session.toggle_play",
            Self::ToggleMute => "media.session.toggle_mute",
            Self::Previous => "media.session.previous",
            Self::Next => "media.session.next",
            Self::PictureInPicture => "media.session.pip",
            Self::Focus => "media.session.focus",
            Self::Dismiss => "media.session.dismiss",
            Self::Seek(_) => "media.session.seek",
        }
    }
}

/// The session the sidebar's media controls show, if any.
///
/// As in Zen, the controls follow media in a tab other than the one on screen: playing media first, then paused media,
/// latest session first. Dismissed sessions are skipped.
pub fn controls_session(model: &BrowserModel) -> Option<&MediaSession> {
    let visible = |session: &&MediaSession| !session.dismissed && Some(session.tab_id) != model.active_tab;
    model
        .media_sessions
        .iter()
        .rev()
        .filter(visible)
        .find(|session| session.is_playing)
        .or_else(|| model.media_sessions.iter().rev().find(visible))
}

/// Applies a control to `session`. Returns false when the control changes nothing in the model (Focus, Previous and
/// Next only reach the engine).
pub fn apply_control(session: &mut MediaSession, control: MediaControl) -> bool {
    match control {
        MediaControl::TogglePlay => {
            session.is_playing = !session.is_playing;
            if session.is_playing {
                session.dismissed = false;
            }
            true
        }
        MediaControl::ToggleMute => {
            session.is_muted = !session.is_muted;
            true
        }
        MediaControl::Dismiss => {
            session.dismissed = true;
            true
        }
        MediaControl::Seek(position) => {
            let end = session.duration_secs.unwrap_or(f32::MAX);
            session.position_secs = position.clamp(0.0, end);
            true
        }
        MediaControl::PictureInPicture | MediaControl::Focus | MediaControl::Previous | MediaControl::Next => false,
    }
}

/// Formats seconds as Zen's media controls do: `m:ss`, or `h:mm:ss` past an hour.
pub fn format_media_time(seconds: f32) -> String {
    let total = if seconds.is_finite() {
        seconds.max(0.0) as u64
    } else {
        0
    };
    let (hours, minutes, secs) = (total / 3600, (total / 60) % 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

/// A sample session for `tab_id`, used by the stub and by `debug.sample_media`.
pub fn sample_session(id: MediaSessionId, tab_id: TabId) -> MediaSession {
    MediaSession {
        id,
        tab_id,
        title: "Ambient Focus Radio".to_string(),
        artist: "Zendia Sample Stream".to_string(),
        is_playing: true,
        is_muted: false,
        position_secs: 142.0,
        duration_secs: Some(360.0),
        has_previous: true,
        has_next: true,
        supports_pip: true,
        dismissed: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_with(sessions: Vec<MediaSession>, active: Option<TabId>) -> BrowserModel {
        BrowserModel {
            media_sessions: sessions,
            active_tab: active,
            ..BrowserModel::default()
        }
    }

    #[test]
    fn controls_skip_the_tab_on_screen() {
        let model = model_with(vec![sample_session(MediaSessionId(1), TabId(3))], Some(TabId(3)));
        assert!(controls_session(&model).is_none());
        let model = model_with(vec![sample_session(MediaSessionId(1), TabId(3))], Some(TabId(1)));
        assert_eq!(controls_session(&model).map(|s| s.id), Some(MediaSessionId(1)));
    }

    #[test]
    fn controls_prefer_playing_media() {
        let mut paused = sample_session(MediaSessionId(2), TabId(4));
        paused.is_playing = false;
        let model = model_with(
            vec![sample_session(MediaSessionId(1), TabId(3)), paused],
            Some(TabId(1)),
        );
        assert_eq!(controls_session(&model).map(|s| s.id), Some(MediaSessionId(1)));
    }

    #[test]
    fn dismissed_sessions_come_back_when_played() {
        let mut session = sample_session(MediaSessionId(1), TabId(3));
        session.is_playing = false;
        apply_control(&mut session, MediaControl::Dismiss);
        let model = model_with(vec![session.clone()], Some(TabId(1)));
        assert!(controls_session(&model).is_none());
        apply_control(&mut session, MediaControl::TogglePlay);
        assert!(!session.dismissed && session.is_playing);
    }

    #[test]
    fn seeking_stays_inside_the_track() {
        let mut session = sample_session(MediaSessionId(1), TabId(3));
        apply_control(&mut session, MediaControl::Seek(9999.0));
        assert_eq!(session.position_secs, 360.0);
        apply_control(&mut session, MediaControl::Seek(-5.0));
        assert_eq!(session.position_secs, 0.0);
    }

    #[test]
    fn times_format_like_zen() {
        assert_eq!(format_media_time(62.4), "1:02");
        assert_eq!(format_media_time(3725.0), "1:02:05");
        assert_eq!(format_media_time(f32::NAN), "0:00");
    }
}
