/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Media controls at the foot of the sidebar (Zen's `ZenMediaController`).
//!
//! The card follows media playing in background tabs (up to 3 stacked cards). It reserves only its collapsed height
//! in the sidebar flow and expands upward over the tab list on hover so the tab strip never shifts.

use crate::ui::chrome::sidebar::try_use_sidebar_state;
use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, focus_ring_border};
use crate::ui::theme::{ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::media::{self, MediaControl};
use zendia_browser_core::model::{MediaSession, TabId};

const BUTTON: f32 = 26.0;
const COLLAPSED_SLOT_HEIGHT: f32 = 42.0;
const STACK_PEEK: f32 = 8.0;
const INFO_HEIGHT: f32 = 48.0;
const PROGRESS_HEIGHT: f32 = 24.0;
const THUMB_SIZE: f32 = 8.0;
const PLAYBACK_TICK: Duration = Duration::from_secs(1);

#[derive(Clone, PartialEq)]
pub struct SidebarMediaControls {
    pub collapsed: bool,
}

impl Component for SidebarMediaControls {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let sidebar_state = try_use_sidebar_state();
        let mut hovered = use_state(|| false);
        let mut cached_sessions = use_state(Vec::<MediaSession>::new);
        let mut was_on_media_tab = use_state(|| false);
        let mut delay_hidden = use_state(|| false);
        let mut delay_gen = use_state(|| 0u64);

        let (live_sessions, tab_favicons, active_tab_has_media) = {
            let model = core.model().read();
            let sessions: Vec<MediaSession> = media::visible_sessions(&model).into_iter().cloned().collect();
            let favicons: Vec<(TabId, Option<String>)> = sessions
                .iter()
                .map(|s| (s.tab_id, model.tab(s.tab_id).and_then(|t| t.favicon.clone())))
                .collect();
            let active_has_media = model.active_tab.is_some_and(|active_id| {
                model
                    .media_sessions
                    .iter()
                    .any(|s| !s.dismissed && s.tab_id == active_id)
            });
            (sessions, favicons, active_has_media)
        };

        // When the user leaves a tab that is playing media, delay revealing the card by `MEDIA_REAPPEAR_DELAY` (500ms),
        // matching Zen's `ZenMediaController`. Initial mounts (including gallery states) appear immediately.
        let prev_on_media = *was_on_media_tab.peek();
        if prev_on_media != active_tab_has_media {
            was_on_media_tab.set(active_tab_has_media);
            if prev_on_media && !active_tab_has_media && !live_sessions.is_empty() {
                delay_hidden.set(true);
                let next_gen = delay_gen.peek().wrapping_add(1);
                delay_gen.set(next_gen);
                spawn(async move {
                    timer(motion::MEDIA_REAPPEAR_DELAY).await;
                    if *delay_gen.peek() == next_gen {
                        delay_hidden.set(false);
                    }
                });
            } else if active_tab_has_media {
                let next_gen = delay_gen.peek().wrapping_add(1);
                delay_gen.set(next_gen);
                delay_hidden.set_if_modified(false);
            }
        }

        if !live_sessions.is_empty() && !self.collapsed {
            if *cached_sessions.peek() != live_sessions {
                cached_sessions.set(live_sessions.clone());
            }
        } else if live_sessions.is_empty() {
            hovered.set_if_modified(false);
        }

        let target_visible = !self.collapsed && !live_sessions.is_empty() && !*delay_hidden.read();
        let appear = use_animation_with_dependencies(&target_visible, move |conf, visible| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to, duration) = if *visible {
                (0.0, 1.0, motion::MEDIA_EXPAND)
            } else {
                (1.0, 0.0, motion::MEDIA_EXIT)
            };
            AnimNum::new(from, to)
                .duration(duration)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let pinned_open = sidebar_state.is_some_and(|state| *state.preview_media_expanded.read());
        let expanded = (*hovered.read() || pinned_open) && target_visible;
        let expand = use_animation_with_dependencies(&expanded, move |conf, open| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *open { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::MEDIA_EXPAND)
                .ease(Ease::InOut)
                .function(motion::ease_out_fn())
        });

        // Hide entirely when the sidebar is collapsed (matching Zen's `zen-media-controls.css`).
        if self.collapsed {
            return rect().into_element();
        }

        let shown = appear.get().value().clamp(0.0, 1.0);
        if !target_visible && shown <= 0.01 {
            return rect().into_element();
        }

        let sessions = if !live_sessions.is_empty() {
            live_sessions
        } else {
            cached_sessions.read().clone()
        };
        if sessions.is_empty() {
            return rect().into_element();
        }

        let open = expand.get().value().clamp(0.0, 1.0);
        let stack_count = sessions.len();
        let slot_height = COLLAPSED_SLOT_HEIGHT + STACK_PEEK * stack_count.saturating_sub(1) as f32;
        let card_height_for = |session: &MediaSession, card_open: f32| {
            let progress_h = if media::has_seekable_duration(session) {
                PROGRESS_HEIGHT * card_open
            } else {
                0.0
            };
            34.0 + INFO_HEIGHT * card_open + progress_h
        };
        let overlay_height = if open > 0.05 && stack_count > 1 {
            sessions
                .iter()
                .enumerate()
                .map(|(idx, s)| card_height_for(s, if idx == 0 { open } else { open * 0.9 }))
                .sum::<f32>()
                + 6.0 * stack_count.saturating_sub(1) as f32
        } else {
            card_height_for(&sessions[0], open) + STACK_PEEK * stack_count.saturating_sub(1) as f32
        };

        // Reserve only the collapsed slot height in the sidebar layout; the floating overlay expands upward from the
        // bottom over the tab list without pushing tabs upward.
        let cards: Vec<Element> = if open > 0.05 && stack_count > 1 {
            // Fanned out vertically above the front card when hovered.
            sessions
                .iter()
                .rev()
                .enumerate()
                .map(|(rev_idx, session)| {
                    let is_front = rev_idx + 1 == stack_count;
                    let favicon = tab_favicons
                        .iter()
                        .find(|(tid, _)| *tid == session.tab_id)
                        .and_then(|(_, fav)| fav.clone());
                    MediaSessionCard {
                        session: session.clone(),
                        favicon,
                        open: if is_front { open } else { open * 0.9 },
                        hovered: *hovered.read(),
                        key: DiffKey::from(&format!("media-card-fan-{}", session.id.0)),
                    }
                    .into_element()
                })
                .collect()
        } else {
            // Stacked behind the front card with peek offset, scale and opacity steps.
            let mut stacked = Vec::with_capacity(stack_count);
            for (level, session) in sessions.iter().enumerate().rev() {
                let favicon = tab_favicons
                    .iter()
                    .find(|(tid, _)| *tid == session.tab_id)
                    .and_then(|(_, fav)| fav.clone());
                let card = MediaSessionCard {
                    session: session.clone(),
                    favicon,
                    open: if level == 0 { open } else { 0.0 },
                    hovered: *hovered.read(),
                    key: DiffKey::from(&format!("media-card-stack-{}", session.id.0)),
                };
                if level == 0 {
                    stacked.push(card.into_element());
                } else {
                    let level_f = level as f32;
                    let scale = (1.0 - 0.04 * level_f).max(0.84);
                    let opacity = (1.0 - 0.3 * level_f).max(0.35);
                    stacked.push(
                        rect()
                            .position(
                                Position::new_absolute()
                                    .bottom(STACK_PEEK * level_f)
                                    .left(0.0)
                                    .right(0.0),
                            )
                            .width(Size::percent(100.0))
                            .height(Size::px(34.0))
                            .scale(scale)
                            .opacity(opacity)
                            .child(card)
                            .into_element(),
                    );
                }
            }
            stacked
        };

        rect()
            .width(Size::fill())
            .height(Size::px(slot_height))
            .padding(Gaps::new(4.0, 2.0, 4.0, 2.0))
            .child(
                rect()
                    .layer(Layer::Relative(12))
                    .position(Position::new_absolute().bottom(0.0).left(0.0).right(0.0))
                    .width(Size::percent(100.0))
                    .height(Size::px(overlay_height))
                    .main_align(Alignment::End)
                    .spacing(6.0)
                    .opacity(shown)
                    .offset_y(10.0 * (1.0 - shown))
                    .on_pointer_over(move |_| hovered.set_if_modified(true))
                    .on_pointer_out(move |_| hovered.set_if_modified(false))
                    .children(cards),
            )
            .into_element()
    }
}

#[derive(Clone, PartialEq)]
struct MediaSessionCard {
    session: MediaSession,
    favicon: Option<String>,
    open: f32,
    hovered: bool,
    key: DiffKey,
}

impl KeyExt for MediaSessionCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for MediaSessionCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let session = &self.session;
        let open = self.open;

        let bg = if theme.scheme.is_dark() {
            Color::from_argb(232, 18, 18, 22)
        } else {
            Color::from_argb(232, 255, 255, 255)
        };
        let border_color = theme.rgba(ThemeRole::BorderSubtle).to_color();

        let focus_button = MediaFocusButton {
            session_id: session.id,
            favicon: self.favicon.clone(),
            is_playing: session.is_playing,
            hovered: self.hovered,
        };

        let controls_row = rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .main_align(Alignment::SpaceBetween)
            .cross_align(Alignment::center())
            .child(focus_button)
            .maybe_child(session.sharing_mic.then(|| {
                MediaButton::new(
                    session,
                    if session.mic_muted {
                        "microphone-blocked"
                    } else {
                        "microphone"
                    },
                    if session.mic_muted {
                        "Unmute microphone"
                    } else {
                        "Mute microphone"
                    },
                    MediaControl::ToggleMic,
                )
                .active(session.mic_muted)
            }))
            .maybe_child(session.sharing_camera.then(|| {
                MediaButton::new(
                    session,
                    "camera",
                    if session.camera_muted {
                        "Enable camera"
                    } else {
                        "Disable camera"
                    },
                    MediaControl::ToggleCamera,
                )
                .active(session.camera_muted)
            }))
            .child(
                MediaButton::new(session, "media-previous", "Previous track", MediaControl::Previous)
                    .disabled(!session.has_previous),
            )
            .child(play_button(session))
            .child(
                MediaButton::new(session, "media-next", "Next track", MediaControl::Next).disabled(!session.has_next),
            )
            .child(MediaButton::new(
                session,
                mute_icon(session),
                if session.is_muted { "Unmute" } else { "Mute" },
                MediaControl::ToggleMute,
            ));

        let info =
            (open > 0.01).then(|| {
                let focus_core = core.clone();
                let session_id = session.id;
                rect()
                    .width(Size::fill())
                    .height(Size::px(INFO_HEIGHT * open))
                    .opacity(open)
                    .overflow(Overflow::Clip)
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .padding(Gaps::new(0.0, 0.0, 0.0, 4.0))
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .spacing(2.0)
                            .cursor(CursorIcon::Pointer)
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Go to the media's tab")
                            .on_press(move |_| {
                                focus_core.invoke_or_log(Command::MediaSessionControl {
                                    session_id,
                                    control: MediaControl::Focus,
                                });
                            })
                            .child(
                                label()
                                    .text(session.title.clone())
                                    .font_size(12.5)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .max_lines(1)
                                    .text_overflow(TextOverflow::Ellipsis)
                                    .color(theme.color(ThemeRole::Text)),
                            )
                            .child(
                                label()
                                    .text(session.artist.clone())
                                    .font_size(11.0)
                                    .max_lines(1)
                                    .text_overflow(TextOverflow::Ellipsis)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            ),
                    )
                    .maybe_child(session.supports_pip.then(|| {
                        MediaButton::new(session, "popup", "Picture in picture", MediaControl::PictureInPicture)
                    }))
                    .child(MediaButton::new(
                        session,
                        "close",
                        "Hide media controls",
                        MediaControl::Dismiss,
                    ))
            });

        let show_progress = open > 0.01 && media::has_seekable_duration(session);
        let progress = show_progress.then(|| MediaProgress {
            session: session.clone(),
            open,
        });

        rect()
            .width(Size::fill())
            .padding(Gaps::new(4.0, 6.0, 4.0, 6.0))
            .corner_radius(CornerRadius::new_all(8.0))
            .background(bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_color),
            )
            .shadow(theme.shadows.float_drop.to_shadow())
            .a11y_role(AccessibilityRole::Group)
            .a11y_alt(format!("Media: {}", session.title))
            .maybe_child(info)
            .maybe_child(progress)
            .child(controls_row)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Tab favicon focus button at the start of the media controls row (Zen's `.zen-media-focus-button`).
#[derive(Clone, PartialEq)]
struct MediaFocusButton {
    session_id: zendia_browser_core::model::MediaSessionId,
    favicon: Option<String>,
    is_playing: bool,
    hovered: bool,
}

impl Component for MediaFocusButton {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut btn_hovered = use_state(|| false);
        let a11y_id = use_hook(AccessibilityId::new_unique);
        let focus = use_focus(a11y_id);

        let session_id = self.session_id;
        let focus_core = core.clone();
        let key_core = core;
        let bg = if *btn_hovered.read() {
            theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color()
        } else {
            Color::TRANSPARENT
        };
        let icon_name = self.favicon.as_deref().unwrap_or("volume-up").to_string();
        let show_playing_badge = self.is_playing && !self.hovered;

        rect()
            .width(Size::px(BUTTON))
            .height(Size::px(BUTTON))
            .corner_radius(CornerRadius::new_all(5.0))
            .background(bg)
            .border(focus_ring_border(focus() != Focus::Not, &theme, Border::new()))
            .center()
            .cursor(CursorIcon::Pointer)
            .a11y_id(a11y_id)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Switch to playing tab")
            .a11y_focusable(true)
            .on_pointer_enter(move |_| btn_hovered.set_if_modified(true))
            .on_pointer_leave(move |_| btn_hovered.set_if_modified(false))
            .on_press(move |_| {
                focus_core.invoke_or_log(Command::MediaSessionControl {
                    session_id,
                    control: MediaControl::Focus,
                });
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if matches!(e.key, Key::Named(NamedKey::Enter)) || e.key == Key::Character(" ".into()) {
                    e.stop_propagation();
                    e.prevent_default();
                    key_core.invoke_or_log(Command::MediaSessionControl {
                        session_id,
                        control: MediaControl::Focus,
                    });
                }
            })
            .child(Icon::new(icon_name).size(14.0).role(ThemeRole::Text))
            .maybe_child(show_playing_badge.then(|| {
                rect()
                    .position(Position::new_absolute().right(2.0).bottom(2.0))
                    .width(Size::px(6.0))
                    .height(Size::px(6.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(theme.color(ThemeRole::Accent))
            }))
    }
}

fn mute_icon(session: &MediaSession) -> &'static str {
    if session.is_muted { "media-mute" } else { "media-unmute" }
}

fn play_button(session: &MediaSession) -> MediaButton {
    if session.is_playing {
        MediaButton::new(session, "media-pause", "Pause", MediaControl::TogglePlay)
    } else {
        MediaButton::new(session, "media-play", "Play", MediaControl::TogglePlay)
    }
}

#[derive(Clone, PartialEq)]
struct MediaButton {
    session: MediaSession,
    icon: &'static str,
    label: &'static str,
    control: MediaControl,
    disabled: bool,
    active: bool,
}

impl MediaButton {
    fn new(session: &MediaSession, icon: &'static str, label: &'static str, control: MediaControl) -> Self {
        Self {
            session: session.clone(),
            icon,
            label,
            control,
            disabled: false,
            active: false,
        }
    }

    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

impl Component for MediaButton {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let a11y_id = use_hook(AccessibilityId::new_unique);
        let focus = use_focus(a11y_id);

        let session_id = self.session.id;
        let control = self.control;
        let disabled = self.disabled;
        let run = move || {
            if !disabled {
                core.invoke_or_log(Command::MediaSessionControl { session_id, control });
            }
        };
        let run_key = run.clone();

        let bg = if self.active && !disabled {
            theme.rgba(ThemeRole::Accent).with_alpha(0.22).to_color()
        } else if *hovered.read() && !disabled {
            theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color()
        } else {
            Color::TRANSPARENT
        };
        let icon_role = if self.active && !disabled {
            ThemeRole::Accent
        } else {
            ThemeRole::Text
        };

        rect()
            .width(Size::px(BUTTON))
            .height(Size::px(BUTTON))
            .corner_radius(CornerRadius::new_all(5.0))
            .background(bg)
            .border(focus_ring_border(focus() != Focus::Not, &theme, Border::new()))
            .center()
            .opacity(if disabled { 0.4 } else { 1.0 })
            .cursor(if disabled {
                CursorIcon::Default
            } else {
                CursorIcon::Pointer
            })
            .a11y_id(a11y_id)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label)
            .a11y_focusable(!disabled)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |_| run())
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if matches!(e.key, Key::Named(NamedKey::Enter)) || e.key == Key::Character(" ".into()) {
                    e.stop_propagation();
                    e.prevent_default();
                    run_key();
                }
            })
            .child(Icon::new(self.icon).size(14.0).role(icon_role))
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.control.command_name())
    }
}

/// Current time, a draggable seek bar with animated hover thumb, and the duration.
#[derive(Clone, PartialEq)]
struct MediaProgress {
    session: MediaSession,
    open: f32,
}

impl Component for MediaProgress {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut bar_area = use_state(|| (0.0f32, 0.0f32));
        let mut bar_hovered = use_state(|| false);
        let mut dragging = use_state(|| false);
        let mut drag_preview = use_state(|| None::<f32>);
        let mut elapsed_offset = use_state(|| 0.0f32);
        let mut tick_key = use_state(|| (0u64, 0u32, false));
        let mut tick_gen = use_state(|| 0u64);

        let duration = self.session.duration_secs.unwrap_or(0.0);
        let current_tick_key = (
            self.session.id.0,
            self.session.position_secs.to_bits(),
            self.session.is_playing,
        );
        if *tick_key.peek() != current_tick_key {
            tick_key.set(current_tick_key);
            elapsed_offset.set_if_modified(0.0);
            let next_gen = tick_gen.peek().wrapping_add(1);
            tick_gen.set(next_gen);
            if self.session.is_playing && duration > 0.0 {
                let base_pos = self.session.position_secs;
                spawn(async move {
                    loop {
                        timer(PLAYBACK_TICK).await;
                        if *tick_gen.peek() != next_gen {
                            break;
                        }
                        if *dragging.peek() {
                            continue;
                        }
                        let next_offset = *elapsed_offset.peek() + 1.0;
                        if base_pos + next_offset >= duration {
                            elapsed_offset.set((duration - base_pos).max(0.0));
                            break;
                        }
                        elapsed_offset.set(next_offset);
                    }
                });
            }
        }

        let show_thumb = *bar_hovered.read() || *dragging.read();
        let thumb_anim = use_animation_with_dependencies(&show_thumb, move |conf, shown| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *shown { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::MEDIA_THUMB)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let thumb_scale = thumb_anim.get().value().clamp(0.0, 1.0);

        let displayed_pos = (*drag_preview.read())
            .unwrap_or(self.session.position_secs + *elapsed_offset.read())
            .clamp(0.0, duration.max(0.0));
        let fraction = if duration > 0.0 {
            (displayed_pos / duration).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let accent = theme.rgba(ThemeRole::Accent).mix(theme.rgba(ThemeRole::Text), 0.3);
        let session_id = self.session.id;
        let (_, bar_width) = *bar_area.read();
        let thumb_left = (bar_width * fraction - THUMB_SIZE * 0.5).clamp(0.0, (bar_width - THUMB_SIZE).max(0.0));

        let down_core = core.clone();
        let move_core = core.clone();
        let up_core = core;

        let time_label = |text: String| {
            label()
                .text(text)
                .font_size(10.5)
                .font_weight(FontWeight::MEDIUM)
                .color(theme.rgba(ThemeRole::Text).with_alpha(0.7).to_color())
        };

        rect()
            .width(Size::fill())
            .height(Size::px(PROGRESS_HEIGHT * self.open))
            .opacity(self.open)
            .overflow(Overflow::Clip)
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .spacing(6.0)
            .padding(Gaps::new(0.0, 4.0, 0.0, 4.0))
            .child(time_label(media::format_media_time(displayed_pos)))
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .height(Size::px(14.0))
                    .main_align(Alignment::center())
                    .cursor(CursorIcon::Pointer)
                    .a11y_role(AccessibilityRole::Slider)
                    .a11y_alt("Seek")
                    .on_sized(move |e: Event<SizedEventData>| {
                        let next = (e.area.min_x(), e.area.width());
                        if *bar_area.peek() != next {
                            bar_area.set(next);
                        }
                    })
                    .on_pointer_enter(move |_| bar_hovered.set_if_modified(true))
                    .on_pointer_leave(move |_| bar_hovered.set_if_modified(false))
                    .on_pointer_down(move |e: Event<PointerEventData>| {
                        if e.button() != Some(MouseButton::Left) {
                            return;
                        }
                        let (x, width) = *bar_area.peek();
                        if width > 0.0 && duration > 0.0 {
                            let at = ((e.global_location().x as f32 - x) / width).clamp(0.0, 1.0);
                            let pos = at * duration;
                            dragging.set(true);
                            drag_preview.set(Some(pos));
                            down_core.invoke_or_log(Command::MediaSessionControl {
                                session_id,
                                control: MediaControl::Seek(pos),
                            });
                        }
                    })
                    .on_global_pointer_move(move |e: Event<PointerEventData>| {
                        if !*dragging.peek() {
                            return;
                        }
                        let (x, width) = *bar_area.peek();
                        if width > 0.0 && duration > 0.0 {
                            let at = ((e.global_location().x as f32 - x) / width).clamp(0.0, 1.0);
                            let pos = at * duration;
                            drag_preview.set(Some(pos));
                            move_core.invoke_or_log(Command::MediaSessionControl {
                                session_id,
                                control: MediaControl::Seek(pos),
                            });
                        }
                    })
                    .on_global_pointer_up(move |e: Event<PointerEventData>| {
                        if !*dragging.peek() {
                            return;
                        }
                        dragging.set(false);
                        let (x, width) = *bar_area.peek();
                        if width > 0.0 && duration > 0.0 {
                            let at = ((e.global_location().x as f32 - x) / width).clamp(0.0, 1.0);
                            up_core.invoke_or_log(Command::MediaSessionControl {
                                session_id,
                                control: MediaControl::Seek(at * duration),
                            });
                        }
                        drag_preview.set(None);
                    })
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::px(4.0))
                            .corner_radius(CornerRadius::new_all(999.0))
                            .background(theme.rgba(ThemeRole::Text).with_alpha(0.2).to_color())
                            .child(
                                rect()
                                    .width(Size::percent(fraction * 100.0))
                                    .height(Size::fill())
                                    .corner_radius(CornerRadius::new_all(999.0))
                                    .background(accent.to_color()),
                            ),
                    )
                    .maybe_child((thumb_scale > 0.01 && bar_width > 0.0).then(|| {
                        rect()
                            .position(Position::new_absolute().left(thumb_left).top(3.0))
                            .width(Size::px(THUMB_SIZE))
                            .height(Size::px(THUMB_SIZE))
                            .corner_radius(CornerRadius::new_all(999.0))
                            .background(theme.color(ThemeRole::Accent))
                            .scale(thumb_scale)
                            .opacity(thumb_scale)
                    })),
            )
            .child(time_label(media::format_media_time(duration)))
    }
}
