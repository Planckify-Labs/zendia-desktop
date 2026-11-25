/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 9 gallery section: Library Downloads (empty, mixed states, filtered), Library Media (opt-in empty state,
//! thumbnail grid of local PNGs and video tile), and the full-size Media Preview overlay.

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::chrome::menus::try_use_menu_state;
use crate::ui::core_view::use_core;
use crate::ui::kit::{KitChip, Pill};
use crate::ui::library::media_preview::try_use_media_preview_state;
use crate::ui::library::sections::downloads::use_provide_downloads_view_state;
use crate::ui::library::try_use_library_state;
use crate::ui::page::{PageArea, PaneMode};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::BrowserCore;
use zendia_browser_core::commands::Command;
use zendia_browser_core::downloads::{current_unix_seconds, sample_mixed_downloads};
use zendia_browser_core::library::LibrarySection;
use zendia_browser_core::library_media::{materialize_sample_media, sample_download_thumbnail_path};
use zendia_browser_core::model::{BrowserModel, LibraryMediaId};
use zendia_module_api::ui::UiTone;

/// Gallery section showcasing Phase 9 Downloads, Media, and Media Preview states.
#[derive(Clone)]
pub struct LibraryMediaSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
    key: DiffKey,
}

impl PartialEq for LibraryMediaSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

impl KeyExt for LibraryMediaSection {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl LibraryMediaSection {
    pub fn new(state_id: impl Into<String>, gallery_core: Arc<GalleryCore>) -> Self {
        Self {
            state_id: state_id.into(),
            gallery_core,
            key: DiffKey::None,
        }
    }
}

fn build_fixture_model(state_id: &str) -> BrowserModel {
    let mut model = super::gallery_sample_model();
    let now = current_unix_seconds();

    match state_id {
        "downloads-empty" => {
            model.downloads.clear();
            model.library_media_enabled = false;
            model.library_media.clear();
            model.library_media_folders.clear();
        }
        "downloads-mixed" | "downloads-filtered" => {
            let thumb = Some(sample_download_thumbnail_path());
            model.downloads = sample_mixed_downloads(now, thumb);
            model.library_media_enabled = false;
            model.library_media.clear();
            model.library_media_folders.clear();
        }
        "media-empty" => {
            model.library_media_enabled = false;
            model.library_media.clear();
            model.library_media_folders.clear();
        }
        "media-grid" | "media-preview" | "media-preview-next" => {
            let (items, folders) = materialize_sample_media(now);
            model.library_media_enabled = true;
            model.library_media = items;
            model.library_media_folders = folders;
        }
        _ => {}
    }

    model
}

impl Component for LibraryMediaSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let library_state = try_use_library_state();
        let preview_state = try_use_media_preview_state();
        let menu_state = try_use_menu_state();
        let initial_query = if self.state_id == "downloads-filtered" {
            "nightly"
        } else {
            ""
        };
        let mut downloads_view = use_provide_downloads_view_state(initial_query, false, Vec::new(), None);
        let location = use_state(|| "https://zendia.local/docs".to_string());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut applied_state = use_state(String::new);

        if *applied_state.peek() != self.state_id {
            applied_state.set(self.state_id.clone());
            let mut fixture = build_fixture_model(&self.state_id);
            fixture.theme = self.gallery_core.model().theme;
            let first_media = fixture.library_media.first().map(|m| m.id);
            let second_media = fixture.library_media.get(1).map(|m| m.id);
            self.gallery_core.stub().replace_model(fixture);

            downloads_view.query.set(if self.state_id == "downloads-filtered" {
                "nightly".to_string()
            } else {
                String::new()
            });
            downloads_view.filters_open.set(false);
            downloads_view.file_groups.set(Vec::new());
            downloads_view.when.set(None);

            if let Some(ms) = menu_state {
                ms.close_all();
            }

            if let Some(ls) = library_state {
                let target_section = if self.state_id.starts_with("downloads") {
                    LibrarySection::Downloads
                } else {
                    LibrarySection::Media
                };
                ls.open(Some(target_section));
            }

            if let Some(ps) = preview_state {
                match self.state_id.as_str() {
                    "media-preview" => {
                        if let Some(id) = first_media {
                            ps.open_item(id);
                        }
                    }
                    "media-preview-next" => {
                        if let Some(id) = second_media.or(first_media) {
                            ps.open_item(id);
                        }
                    }
                    _ => ps.close(),
                }
            }
        }

        let status_label = match self.state_id.as_str() {
            "downloads-empty" => "Downloads · empty state",
            "downloads-mixed" => "Downloads · active, paused, done, failed & cancelled",
            "downloads-filtered" => "Downloads · filtered by query \"nightly\"",
            "media-empty" => "Media · opt-in card over skeleton grid",
            "media-grid" => "Media · 3-column grid of local PNGs + video tile",
            "media-preview" => "Media preview · full-size image overlay",
            "media-preview-next" => "Media preview · stepped forward with arrow to video",
            _ => "Library · Downloads & Media",
        };

        let sample_dl_core = core.clone();
        let clear_dl_core = core.clone();
        let preview_open_core = core.clone();
        let preview_step_core = core.clone();
        let is_downloads = self.state_id.starts_with("downloads");
        let first_media_id = core
            .model()
            .read()
            .library_media
            .first()
            .map(|m| m.id)
            .unwrap_or(LibraryMediaId(1));

        let controls = rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .child(Pill::new(status_label).tone(UiTone::Accent))
            .maybe_child(is_downloads.then(|| {
                KitChip::new("Start Sample Download")
                    .icon("downloads")
                    .on_press(move |_| {
                        sample_dl_core.invoke_or_log(Command::DebugSampleDownload);
                    })
            }))
            .maybe_child(is_downloads.then(|| {
                KitChip::new("Clear Finished").icon("trash").on_press(move |_| {
                    clear_dl_core.invoke_or_log(Command::DownloadClearFinished);
                })
            }))
            .maybe_child((!is_downloads).then(|| {
                KitChip::new("Open Preview").icon("new-tab-image").on_press(move |_| {
                    preview_open_core.invoke_or_log(Command::LibraryMediaPreviewOpen {
                        media_id: first_media_id,
                    });
                })
            }))
            .maybe_child((!is_downloads).then(|| {
                KitChip::new("Next Media →").icon("arrow-right").on_press(move |_| {
                    preview_step_core.invoke_or_log(Command::LibraryMediaPreviewStep { delta: 1 });
                })
            }));

        let shell = rect()
            .width(Size::fill())
            .height(Size::px(672.0))
            .corner_radius(CornerRadius::new_all(theme.radii.card))
            .overflow(Overflow::Clip)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .child(
                WindowShell::new(location, location_a11y_id, PageArea::new(PaneMode::Placeholder))
                    .interactive_window_controls(false),
            );

        rect().width(Size::fill()).spacing(12.0).child(controls).child(shell)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
