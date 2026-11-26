/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 4 gallery section for compact mode, media controls, the download animation and toasts.

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::chrome::sidebar::{SidebarViewState, try_use_sidebar_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::{KitChip, Pill};
use crate::ui::page::download_arc::{ArcFlight, DownloadArcLayer, DownloadArcState, try_use_download_arc};
use crate::ui::page::{PageArea, PaneMode};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::commands::Command;
use zendia_browser_core::media;
use zendia_browser_core::model::{BrowserModel, CompactHide, MediaSessionId, SpaceId, TabId, ToolbarLayout};
use zendia_browser_core::stub::sample_update_notification;
use zendia_browser_core::toasts::{Toast, ToastAction, ToastId, messages};
use zendia_module_api::ui::UiTone;

const DOCS: TabId = TabId(12);
const RELEASES: TabId = TabId(9);
/// Gallery toasts stay up until the state changes.
const GALLERY_TOAST_MS: u64 = 10 * 60 * 1000;
/// Where the pinned download flight sits mid-way.
const PINNED_ARC_PROGRESS: f32 = 0.42;

#[derive(Clone)]
pub struct LayoutsSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
}

impl PartialEq for LayoutsSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

fn toast(id: u64, message: &str, action: Option<(&str, &str)>) -> Toast {
    Toast {
        id: ToastId(id),
        message: message.to_string(),
        description: None,
        action: action.map(|(label, command)| ToastAction {
            label: label.to_string(),
            command: command.to_string(),
            command_args: Vec::new(),
        }),
        timeout_ms: GALLERY_TOAST_MS,
        bounce_seq: 0,
    }
}

fn add_media(model: &mut BrowserModel, playing: bool, muted: bool) {
    let mut session = media::sample_session(MediaSessionId(1), RELEASES);
    session.is_playing = playing;
    session.is_muted = muted;
    model.media_sessions = vec![session];
}

fn configure_fixture(
    state_id: &str,
    gallery_core: &GalleryCore,
    sidebar_state: Option<SidebarViewState>,
    arc: Option<DownloadArcState>,
) {
    if let Some(state) = sidebar_state {
        state.reset();
    }
    if let Some(arc) = arc {
        let mut flights = arc.flights;
        flights.set(Vec::new());
    }
    let mut model = super::gallery_sample_model();
    model.active_tab = Some(DOCS);
    if let Some(window) = model.windows.first_mut() {
        window.active_tab = Some(DOCS);
    }
    if let Some(space) = model.spaces.iter_mut().find(|space| space.id == SpaceId(1)) {
        space.last_active_tab = Some(DOCS);
    }

    let mut reveal = false;
    let mut media_expanded = false;
    match state_id {
        "compact-revealed" => {
            model.layout.compact_mode = true;
            reveal = true;
        }
        "compact-toolbar" => {
            model.layout.toolbar_layout = ToolbarLayout::Multiple;
            model.layout.compact_mode = true;
            model.layout.compact_hide = CompactHide::Toolbar;
        }
        "media-playing" => add_media(&mut model, true, false),
        "media-expanded" => {
            add_media(&mut model, true, false);
            media_expanded = true;
        }
        "media-paused-muted" => {
            add_media(&mut model, false, true);
            media_expanded = true;
        }
        "media-with-notification" => {
            add_media(&mut model, true, false);
            model.sidebar_notifications.push(sample_update_notification());
        }
        "download-arc" => {}
        "toast" => model.toasts = vec![toast(1, messages::YANK_COPIED, None)],
        "toast-action" => model.toasts = vec![toast(1, messages::TABS_CLOSED, Some(("Undo", "tab.reopen_closed")))],
        "toasts-stacked" => {
            model.toasts = vec![
                toast(1, messages::COPIED_CURRENT_URL, None),
                toast(2, messages::BACKGROUND_TAB_OPENED, None),
            ]
        }
        _ => model.layout.compact_mode = true,
    }
    gallery_core.stub().replace_model(model);

    if let Some(state) = sidebar_state {
        // Pinned rather than hovered out, so the pointer resting elsewhere does not slide it back.
        let mut pinned = state.compact_pinned;
        pinned.set(reveal);
        let mut expanded = state.preview_media_expanded;
        expanded.set(media_expanded);
    }
    if state_id == "download-arc"
        && let Some(arc) = arc
    {
        let end = (*arc.anchor.peek()).unwrap_or((60.0, 900.0));
        let mut flights = arc.flights;
        flights.set(vec![ArcFlight {
            id: 1,
            start: (980.0, 520.0),
            end,
            viewport_height: 1000.0,
        }]);
    }
}

impl Component for LayoutsSection {
    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.state_id)
    }

    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let sidebar_state = try_use_sidebar_state();
        let arc = try_use_download_arc();

        let mut applied_state = use_state(String::new);
        if *applied_state.peek() != self.state_id {
            applied_state.set(self.state_id.clone());
            configure_fixture(&self.state_id, &self.gallery_core, sidebar_state, arc);
        }

        let (summary, active_url) = {
            let model = core.model().read();
            let summary = if model.layout.compact_mode {
                format!("Compact mode · hides {}", model.layout.compact_hide.as_str())
            } else if !model.media_sessions.is_empty() {
                "Media playing in a background tab".to_string()
            } else if !model.toasts.is_empty() {
                format!("{} toast(s)", model.toasts.len())
            } else {
                "Download animation".to_string()
            };
            (
                summary,
                model.active_tab().map(|tab| tab.url.clone()).unwrap_or_default(),
            )
        };

        let mut location = use_state(|| active_url.clone());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut shown_url = use_state(|| active_url.clone());
        if *shown_url.peek() != active_url && !location_a11y_id.is_focused() {
            shown_url.set(active_url.clone());
            location.set(active_url.clone());
        }

        let chip = |label: &'static str, command: Command| {
            let core = core.clone();
            KitChip::new(label)
                .on_press(move |_| {
                    core.invoke_or_log(command.clone());
                })
                .into_element()
        };
        let reset_core = Arc::clone(&self.gallery_core);
        let reset_state_id = self.state_id.clone();
        let launch_arc = arc;

        let info_strip = rect()
            .width(Size::fill())
            .padding(Gaps::new(6.0, 12.0, 6.0, 12.0))
            .direction(Direction::Horizontal)
            .content(Content::wrap())
            .cross_align(Alignment::center())
            .main_align(Alignment::SpaceBetween)
            .spacing(theme.spacing.sm)
            .background(theme.color(ThemeRole::SurfaceRaised))
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .child(Pill::new(summary).tone(UiTone::Accent))
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(theme.spacing.xs)
                    .child(chip("Toggle Compact", Command::LayoutToggleCompactMode))
                    .child(chip("Pin Sidebar Out", Command::LayoutCompactRevealSidebar))
                    .child(chip("Sample Media", Command::DebugSampleMedia))
                    .child(chip(
                        "Show Toast",
                        Command::ToastShow {
                            message: messages::YANK_COPIED.to_string(),
                            timeout_ms: None,
                            action_label: None,
                            action_command: None,
                        },
                    ))
                    .child(KitChip::new("Start Download").on_press(move |_| {
                        if let Some(arc) = launch_arc {
                            arc.launch((980.0, 520.0), (60.0, 900.0));
                        }
                    }))
                    .child(KitChip::new("Reset State").on_press(move |_| {
                        configure_fixture(&reset_state_id, &reset_core, sidebar_state, arc);
                    })),
            );

        let pinned_progress = (self.state_id == "download-arc").then_some(PINNED_ARC_PROGRESS);
        let page = PageArea::new(PaneMode::Placeholder);

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .padding(Gaps::new_all(12.0))
            .spacing(10.0)
            .child(info_strip)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .corner_radius(CornerRadius::new_all(10.0))
                    .overflow(Overflow::Clip)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.color(ThemeRole::Border)),
                    )
                    .child(WindowShell::new(location, location_a11y_id, page).interactive_window_controls(false)),
            )
            .child(DownloadArcLayer { pinned_progress })
    }
}
