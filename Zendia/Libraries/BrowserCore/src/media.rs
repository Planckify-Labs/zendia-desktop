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

/// Maximum number of background media sessions stacked in the sidebar controls card (matches Zen's `MAX_STACKED_CARDS`).
pub const MAX_STACKED_CARDS: usize = 3;

/// Duration threshold in seconds above which media is treated as a livestream and its seek bar is hidden.
pub const LIVESTREAM_DURATION_THRESHOLD_SECS: f32 = 900_000.0;

/// What a media control button does to a session.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MediaControl {
    TogglePlay,
    ToggleMute,
    ToggleMic,
    ToggleCamera,
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
            Self::ToggleMic => "media.session.toggle_mic",
            Self::ToggleCamera => "media.session.toggle_camera",
            Self::Previous => "media.session.previous",
            Self::Next => "media.session.next",
            Self::PictureInPicture => "media.session.pip",
            Self::Focus => "media.session.focus",
            Self::Dismiss => "media.session.dismiss",
            Self::Seek(_) => "media.session.seek",
        }
    }
}

/// Up to [`MAX_STACKED_CARDS`] background media sessions to render in the sidebar media controls stack.
///
/// Playing sessions (or active mic/camera shares) come first, then paused sessions, latest first. Sessions belonging to
/// the active tab on screen or explicitly dismissed are omitted.
pub fn visible_sessions(model: &BrowserModel) -> Vec<&MediaSession> {
    let eligible = |session: &&MediaSession| !session.dismissed && Some(session.tab_id) != model.active_tab;
    let is_active = |session: &&MediaSession| session.is_playing || session.sharing_mic || session.sharing_camera;

    let playing = model
        .media_sessions
        .iter()
        .rev()
        .filter(eligible)
        .filter(|s| is_active(s));
    let paused = model
        .media_sessions
        .iter()
        .rev()
        .filter(eligible)
        .filter(|s| !is_active(s));

    playing.chain(paused).take(MAX_STACKED_CARDS).collect()
}

/// The primary session the sidebar's media controls show at the front of the stack, if any.
pub fn controls_session(model: &BrowserModel) -> Option<&MediaSession> {
    visible_sessions(model).into_iter().next()
}

/// Returns whether `session` has a finite, bounded duration suitable for rendering the seek bar.
pub fn has_seekable_duration(session: &MediaSession) -> bool {
    session
        .duration_secs
        .is_some_and(|d| d.is_finite() && d > 0.0 && d < LIVESTREAM_DURATION_THRESHOLD_SECS)
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
        MediaControl::ToggleMic => {
            session.mic_muted = !session.mic_muted;
            true
        }
        MediaControl::ToggleCamera => {
            session.camera_muted = !session.camera_muted;
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
        sharing_mic: false,
        sharing_camera: false,
        mic_muted: false,
        camera_muted: false,
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
    fn visible_sessions_stacks_playing_first_up_to_three() {
        let mut s1 = sample_session(MediaSessionId(1), TabId(2));
        s1.is_playing = false;
        let s2 = sample_session(MediaSessionId(2), TabId(3));
        let mut s3 = sample_session(MediaSessionId(3), TabId(4));
        s3.is_playing = false;
        let s4 = sample_session(MediaSessionId(4), TabId(5));
        let s5 = sample_session(MediaSessionId(5), TabId(6));
        let model = model_with(vec![s1, s2, s3, s4, s5], Some(TabId(1)));
        let ids: Vec<_> = visible_sessions(&model).into_iter().map(|s| s.id.0).collect();
        assert_eq!(ids, vec![5, 4, 2]);
    }

    #[test]
    fn livestream_and_missing_duration_hide_progress() {
        let mut session = sample_session(MediaSessionId(1), TabId(2));
        assert!(has_seekable_duration(&session));
        session.duration_secs = None;
        assert!(!has_seekable_duration(&session));
        session.duration_secs = Some(0.0);
        assert!(!has_seekable_duration(&session));
        session.duration_secs = Some(LIVESTREAM_DURATION_THRESHOLD_SECS);
        assert!(!has_seekable_duration(&session));
    }

    #[test]
    fn mic_and_camera_mute_toggles_update_session() {
        let mut session = sample_session(MediaSessionId(1), TabId(2));
        session.sharing_mic = true;
        session.sharing_camera = true;
        assert!(apply_control(&mut session, MediaControl::ToggleMic));
        assert!(session.mic_muted);
        assert!(apply_control(&mut session, MediaControl::ToggleCamera));
        assert!(session.camera_muted);
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
