/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Media controls at the foot of the sidebar (Zen's `ZenMediaController`).
//!
//! The card follows media playing in a tab other than the one on screen. At rest it shows the playback buttons;
//! hovering it reveals the title, artist, picture-in-picture, close and the progress bar, with Zen's 0.3 s easing.

use crate::ui::chrome::sidebar::try_use_sidebar_state;
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::theme::{ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::media::{self, MediaControl};
use zendia_browser_core::model::MediaSession;

const EXPAND: Duration = Duration::from_millis(300);
const APPEAR: Duration = Duration::from_millis(220);
const BUTTON: f32 = 26.0;
const INFO_HEIGHT: f32 = 50.0;
const PROGRESS_HEIGHT: f32 = 22.0;

#[derive(Clone, PartialEq)]
pub struct SidebarMediaControls {
    pub collapsed: bool,
}

impl Component for SidebarMediaControls {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let sidebar_state = try_use_sidebar_state();
        let mut hovered = use_state(|| false);

        let session = {
            let model = core.model().read();
            media::controls_session(&model).cloned()
        };
        let session_key = session.as_ref().map(|s| s.id.0);

        let appear = use_animation_with_dependencies(&session_key, move |conf, key| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if key.is_some() { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(APPEAR)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let pinned_open = sidebar_state.is_some_and(|state| *state.preview_media_expanded.read());
        let expanded = (*hovered.read() || pinned_open) && !self.collapsed;
        let expand = use_animation_with_dependencies(&expanded, move |conf, open| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *open { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(EXPAND)
                .ease(Ease::InOut)
                .function(motion::ease_out_fn())
        });

        let Some(session) = session else {
            return rect().into_element();
        };
        let shown = appear.get().value().clamp(0.0, 1.0);
        let open = expand.get().value().clamp(0.0, 1.0);

        let bg = if theme.scheme.is_dark() {
            Color::from_argb(220, 14, 14, 18)
        } else {
            Color::from_argb(200, 255, 255, 255)
        };

        let buttons = if self.collapsed {
            rect()
                .spacing(2.0)
                .cross_align(Alignment::center())
                .child(play_button(&session))
                .child(MediaButton::new(
                    &session,
                    mute_icon(&session),
                    "Mute",
                    MediaControl::ToggleMute,
                ))
                .into_element()
        } else {
            rect()
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .main_align(Alignment::SpaceBetween)
                .cross_align(Alignment::center())
                .child(
                    MediaButton::new(&session, "media-previous", "Previous track", MediaControl::Previous)
                        .disabled(!session.has_previous),
                )
                .child(play_button(&session))
                .child(
                    MediaButton::new(&session, "media-next", "Next track", MediaControl::Next)
                        .disabled(!session.has_next),
                )
                .child(MediaButton::new(
                    &session,
                    mute_icon(&session),
                    "Mute",
                    MediaControl::ToggleMute,
                ))
                .into_element()
        };

        let info =
            (!self.collapsed && open > 0.01).then(|| {
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
                        MediaButton::new(&session, "popup", "Picture in picture", MediaControl::PictureInPicture)
                    }))
                    .child(MediaButton::new(
                        &session,
                        "close",
                        "Hide media controls",
                        MediaControl::Dismiss,
                    ))
            });

        let progress = (!self.collapsed && open > 0.01).then(|| MediaProgress {
            session: session.clone(),
            open,
        });

        rect()
            .width(Size::fill())
            .padding(Gaps::new(4.0, 2.0, 4.0, 2.0))
            .opacity(shown)
            .offset_y(8.0 * (1.0 - shown))
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new(4.0, 6.0, 4.0, 6.0))
                    .corner_radius(CornerRadius::new_all(8.0))
                    .background(bg)
                    .a11y_role(AccessibilityRole::Group)
                    .a11y_alt(format!("Media: {}", session.title))
                    .on_pointer_enter(move |_| hovered.set(true))
                    .on_pointer_leave(move |_| hovered.set(false))
                    .maybe_child(info)
                    .maybe_child(progress)
                    .child(buttons),
            )
            .into_element()
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
}

impl MediaButton {
    fn new(session: &MediaSession, icon: &'static str, label: &'static str, control: MediaControl) -> Self {
        Self {
            session: session.clone(),
            icon,
            label,
            control,
            disabled: false,
        }
    }

    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
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

        let bg = if *hovered.read() && !disabled {
            theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color()
        } else {
            Color::TRANSPARENT
        };
        let mut button = rect()
            .width(Size::px(BUTTON))
            .height(Size::px(BUTTON))
            .corner_radius(CornerRadius::new_all(5.0))
            .background(bg)
            .center()
            .opacity(if disabled { 0.4 } else { 1.0 })
            .a11y_id(a11y_id)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label)
            .a11y_focusable(!disabled)
            .on_pointer_enter(move |_| hovered.set(true))
            .on_pointer_leave(move |_| hovered.set(false))
            .on_press(move |_| run())
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if matches!(e.key, Key::Named(NamedKey::Enter)) || e.key == Key::Character(" ".into()) {
                    run_key();
                }
            })
            .child(Icon::new(self.icon).size(14.0).role(ThemeRole::Text));
        if focus() != Focus::Not {
            button = button.border(
                Border::new()
                    .width(2.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::FocusRing)),
            );
        }
        button
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.control.command_name())
    }
}

/// Current time, a seekable bar and the duration. Clicking the bar seeks.
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

        let duration = self.session.duration_secs.unwrap_or(0.0);
        let fraction = if duration > 0.0 {
            (self.session.position_secs / duration).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let accent = theme.rgba(ThemeRole::Accent).mix(theme.rgba(ThemeRole::Text), 0.3);
        let session_id = self.session.id;
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
            .child(time_label(media::format_media_time(self.session.position_secs)))
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .height(Size::px(12.0))
                    .main_align(Alignment::center())
                    .a11y_role(AccessibilityRole::Slider)
                    .a11y_alt("Seek")
                    .on_sized(move |e: Event<SizedEventData>| {
                        let next = (e.area.min_x(), e.area.width());
                        if *bar_area.peek() != next {
                            bar_area.set(next);
                        }
                    })
                    .on_mouse_down(move |e: Event<MouseEventData>| {
                        let (x, width) = *bar_area.peek();
                        if width > 0.0 && duration > 0.0 {
                            let at = ((e.global_location.x as f32 - x) / width).clamp(0.0, 1.0);
                            core.invoke_or_log(Command::MediaSessionControl {
                                session_id,
                                control: MediaControl::Seek(at * duration),
                            });
                        }
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
                    ),
            )
            .child(time_label(media::format_media_time(duration)))
    }
}
