/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stub handling for Phase 9's Downloads and Media library commands.

use super::{StubBackend, StubState};
use crate::commands::{Command, CommandError, CommandResult};
use crate::downloads::{self, current_unix_seconds};
use crate::events::{Event, ViewRequest};
use crate::library_media::{self, LibraryMediaKind, materialize_sample_media, media_from_downloads, merge_media_items};
use crate::model::{Download, DownloadId, DownloadStatus, LibraryMediaId};

/// Sample templates cycled when `debug.sample_download` is invoked.
const SAMPLE_DOWNLOAD_TEMPLATES: &[(&str, &str, u64, u64)] = &[
    (
        "zendia-release-notes.pdf",
        "https://releases.zendia.org/docs/zendia-release-notes.pdf",
        4_200_000,
        1_120_000,
    ),
    (
        "aurora-workspace.png",
        "https://media.zendia.org/wallpapers/aurora-workspace.png",
        2_800_000,
        960_000,
    ),
    (
        "zendia-toolkit-0.9.zip",
        "https://cdn.zendia.org/packages/zendia-toolkit-0.9.zip",
        5_600_000,
        1_400_000,
    ),
    (
        "walkthrough-clip.mp4",
        "https://clips.zendia.org/walkthrough-clip.mp4",
        6_400_000,
        1_600_000,
    ),
];

impl StubBackend {
    /// Starts a simulated sample download that advances via [`Command::DownloadTick`].
    pub(super) fn start_sample_download(&self) -> CommandResult {
        let now = current_unix_seconds();
        let download = {
            let mut state = self.lock_state();
            let id = DownloadId(state.model.downloads.iter().map(|d| d.id.0).max().unwrap_or(0) + 1);
            let idx = (id.0.saturating_sub(1) as usize) % SAMPLE_DOWNLOAD_TEMPLATES.len();
            let (base_name, url, total, speed) = SAMPLE_DOWNLOAD_TEMPLATES[idx];
            let filename = if state.model.downloads.iter().any(|d| d.filename == base_name) {
                match base_name.rsplit_once('.') {
                    Some((stem, ext)) => format!("{stem}-{}.{ext}", id.0),
                    None => format!("{base_name}-{}", id.0),
                }
            } else {
                base_name.to_string()
            };
            let target_path = if downloads::can_draw_thumbnail(&filename) {
                Some(library_media::sample_download_thumbnail_path())
            } else {
                Some(format!("/home/user/Downloads/{filename}"))
            };
            let download = Download {
                id,
                filename,
                url: url.to_string(),
                bytes_received: 0,
                total_bytes: Some(total),
                status: DownloadStatus::InProgress,
                target_path,
                speed_bytes_per_sec: Some(speed),
                updated_at_unix: now,
                file_exists: true,
                open_when_done: false,
                simulated: true,
            };
            state.model.downloads.push(download.clone());
            download
        };
        self.bus.publish(Event::DownloadStarted { download });
        Ok(())
    }

    /// Handles Phase 9 Downloads and Media commands.
    pub(super) fn handle_library_media_command(&self, command: Command) -> CommandResult {
        let name = command.name().to_string();
        match command {
            Command::DownloadOpen { download_id } => {
                let download = self.find_download(&name, download_id)?;
                if download.status == DownloadStatus::InProgress {
                    return self.handle_library_media_command(Command::DownloadToggleOpenWhenDone { download_id });
                }
                if !downloads::has_file(&download) {
                    return Err(CommandError::InvalidArgument {
                        command: name,
                        message: format!("download `{}` has no completed file to open", download.filename),
                    });
                }
                eprintln!(
                    "zendia: stub download.open `{}` ({:?})",
                    download.filename, download.target_path
                );
                self.show_toast(format!("Opening {}", download.filename), None, None);
                Ok(())
            }
            Command::DownloadShowInFolder { download_id } => {
                let download = self.find_download(&name, download_id)?;
                if !downloads::has_file(&download) {
                    return Err(CommandError::InvalidArgument {
                        command: name,
                        message: format!("download `{}` has no completed file on disk", download.filename),
                    });
                }
                eprintln!(
                    "zendia: stub download.show_in_folder `{}` ({:?})",
                    download.filename, download.target_path
                );
                self.show_toast(format!("Showing {} in folder", download.filename), None, None);
                Ok(())
            }
            Command::DownloadPause { download_id } => {
                let updated = {
                    let mut state = self.lock_state();
                    let download = state
                        .model
                        .downloads
                        .iter_mut()
                        .find(|d| d.id == download_id)
                        .ok_or_else(|| not_found(&name, "download"))?;
                    if download.status != DownloadStatus::InProgress {
                        return Ok(());
                    }
                    download.status = DownloadStatus::Paused;
                    download.speed_bytes_per_sec = None;
                    download.clone()
                };
                self.bus.publish(Event::DownloadUpdated { download: updated });
                Ok(())
            }
            Command::DownloadResume { download_id } => {
                let updated = {
                    let mut state = self.lock_state();
                    let download = state
                        .model
                        .downloads
                        .iter_mut()
                        .find(|d| d.id == download_id)
                        .ok_or_else(|| not_found(&name, "download"))?;
                    if download.status != DownloadStatus::Paused {
                        return Ok(());
                    }
                    download.status = DownloadStatus::InProgress;
                    download.speed_bytes_per_sec = Some(1_280_000);
                    download.updated_at_unix = current_unix_seconds();
                    download.clone()
                };
                self.bus.publish(Event::DownloadUpdated { download: updated });
                Ok(())
            }
            Command::DownloadRetry { download_id } => {
                let updated = {
                    let mut state = self.lock_state();
                    let download = state
                        .model
                        .downloads
                        .iter_mut()
                        .find(|d| d.id == download_id)
                        .ok_or_else(|| not_found(&name, "download"))?;
                    if !downloads::can_retry(download) {
                        return Ok(());
                    }
                    download.bytes_received = 0;
                    download.status = DownloadStatus::InProgress;
                    download.speed_bytes_per_sec = Some(1_450_000);
                    download.updated_at_unix = current_unix_seconds();
                    download.file_exists = true;
                    download.simulated = true;
                    download.clone()
                };
                self.bus.publish(Event::DownloadUpdated { download: updated });
                Ok(())
            }
            Command::DownloadCancel { download_id } => {
                let updated = {
                    let mut state = self.lock_state();
                    let download = state
                        .model
                        .downloads
                        .iter_mut()
                        .find(|d| d.id == download_id)
                        .ok_or_else(|| not_found(&name, "download"))?;
                    if !downloads::can_cancel(download) {
                        return Ok(());
                    }
                    download.status = DownloadStatus::Cancelled;
                    download.speed_bytes_per_sec = None;
                    download.open_when_done = false;
                    download.file_exists = false;
                    download.clone()
                };
                self.bus.publish(Event::DownloadUpdated { download: updated });
                Ok(())
            }
            Command::DownloadRemove { download_id } => {
                {
                    let mut state = self.lock_state();
                    let before = state.model.downloads.len();
                    state.model.downloads.retain(|d| d.id != download_id);
                    if state.model.downloads.len() == before {
                        return Err(not_found(&name, "download"));
                    }
                }
                self.bus.publish(Event::DownloadRemoved { download_id });
                Ok(())
            }
            Command::DownloadClearFinished => {
                let removed = {
                    let mut state = self.lock_state();
                    let before = state.model.downloads.len();
                    state.model.downloads.retain(|d| !downloads::is_finished(d));
                    before - state.model.downloads.len()
                };
                if removed > 0 {
                    self.bus.publish(Event::DownloadsClearedFinished);
                }
                Ok(())
            }
            Command::DownloadCopyLink { download_id } => {
                let download = self.find_download(&name, download_id)?;
                self.bus.publish(Event::ClipboardCopyRequested { text: download.url });
                self.show_toast("Copied download link".to_string(), None, None);
                Ok(())
            }
            Command::DownloadToggleOpenWhenDone { download_id } => {
                let updated = {
                    let mut state = self.lock_state();
                    let download = state
                        .model
                        .downloads
                        .iter_mut()
                        .find(|d| d.id == download_id)
                        .ok_or_else(|| not_found(&name, "download"))?;
                    if download.status != DownloadStatus::InProgress {
                        return Ok(());
                    }
                    download.open_when_done = !download.open_when_done;
                    download.clone()
                };
                self.bus.publish(Event::DownloadUpdated { download: updated });
                Ok(())
            }
            Command::DownloadTick => {
                let (updates, completed_media, media_enabled) = {
                    let mut state = self.lock_state();
                    let mut updates = Vec::new();
                    let mut completed_media = false;
                    let now = current_unix_seconds();
                    for download in &mut state.model.downloads {
                        if !download.simulated || download.status != DownloadStatus::InProgress {
                            continue;
                        }
                        let speed = download.speed_bytes_per_sec.unwrap_or(1_200_000);
                        // Each tick represents ~250 ms of transfer.
                        let step = (speed / 4).max(200_000);
                        let total = download.total_bytes.unwrap_or(4_000_000);
                        download.bytes_received = (download.bytes_received + step).min(total);
                        if download.bytes_received >= total {
                            download.status = DownloadStatus::Completed;
                            download.speed_bytes_per_sec = None;
                            download.file_exists = true;
                            download.updated_at_unix = now;
                            if library_media::media_kind_of(&download.filename).is_some() {
                                completed_media = true;
                            }
                        }
                        updates.push(download.clone());
                    }
                    (updates, completed_media, state.model.library_media_enabled)
                };
                for download in updates {
                    let open_now = download.status == DownloadStatus::Completed && download.open_when_done;
                    let filename = download.filename.clone();
                    self.bus.publish(Event::DownloadUpdated { download });
                    if open_now {
                        self.show_toast(format!("Opening {filename}"), None, None);
                    }
                }
                if completed_media && media_enabled {
                    self.refresh_library_media();
                }
                Ok(())
            }
            Command::LibraryMediaSetEnabled { enabled } => {
                if enabled {
                    let now = current_unix_seconds();
                    let (items, folders) = {
                        let mut state = self.lock_state();
                        state.model.library_media_enabled = true;
                        let (base_items, folders) = materialize_sample_media(now);
                        let dl_items = media_from_downloads(&state.model.downloads);
                        let merged = merge_media_items(base_items, dl_items);
                        state.model.library_media = merged.clone();
                        state.model.library_media_folders = folders.clone();
                        (merged, folders)
                    };
                    self.bus.publish(Event::LibraryMediaChanged {
                        enabled: true,
                        items,
                        folders,
                    });
                } else {
                    {
                        let mut state = self.lock_state();
                        state.model.library_media_enabled = false;
                        state.model.library_media.clear();
                        state.model.library_media_folders.clear();
                    }
                    self.request_view(ViewRequest::LibraryMediaPreviewClose);
                    self.bus.publish(Event::LibraryMediaChanged {
                        enabled: false,
                        items: Vec::new(),
                        folders: Vec::new(),
                    });
                }
                Ok(())
            }
            Command::LibraryMediaRefresh => {
                self.refresh_library_media();
                Ok(())
            }
            Command::LibraryMediaPreviewOpen { media_id } => {
                let _ = self.find_media(&name, media_id)?;
                self.request_view(ViewRequest::LibraryMediaPreviewOpen { media_id });
                Ok(())
            }
            Command::LibraryMediaPreviewClose => {
                self.request_view(ViewRequest::LibraryMediaPreviewClose);
                Ok(())
            }
            Command::LibraryMediaPreviewStep { delta } => {
                self.request_view(ViewRequest::LibraryMediaPreviewStep { delta });
                Ok(())
            }
            Command::LibraryMediaOpen { media_id } => {
                let item = self.find_media(&name, media_id)?;
                let opener = library_media::default_opener_name(item.kind);
                eprintln!(
                    "zendia: stub library.media.open `{}` ({}) in {opener}",
                    item.name, item.path
                );
                let kind_word = match item.kind {
                    LibraryMediaKind::Image => "image",
                    LibraryMediaKind::Video => "video",
                    LibraryMediaKind::File => "file",
                };
                self.show_toast(format!("Opening {kind_word} {} in {opener}", item.name), None, None);
                Ok(())
            }
            Command::LibraryMediaShowInFolder { media_id } => {
                let item = self.find_media(&name, media_id)?;
                eprintln!(
                    "zendia: stub library.media.show_in_folder `{}` ({})",
                    item.name, item.path
                );
                self.show_toast(format!("Showing {} in {}", item.name, item.folder), None, None);
                Ok(())
            }
            Command::LibraryMediaCopy { media_id } => {
                let item = self.find_media(&name, media_id)?;
                let copied = if item.path.is_empty() {
                    item.name.clone()
                } else {
                    item.path.clone()
                };
                self.bus.publish(Event::ClipboardCopyRequested { text: copied });
                self.show_toast(format!("Copied {}", item.name), None, None);
                Ok(())
            }
            other => Err(CommandError::UnknownCommand(other.name().to_string())),
        }
    }

    fn refresh_library_media(&self) {
        let now = current_unix_seconds();
        let (enabled, items, folders) = {
            let mut state = self.lock_state();
            if !state.model.library_media_enabled {
                return;
            }
            let (base_items, folders) = materialize_sample_media(now);
            let dl_items = media_from_downloads(&state.model.downloads);
            let merged = merge_media_items(base_items, dl_items);
            state.model.library_media = merged.clone();
            state.model.library_media_folders = folders.clone();
            (true, merged, folders)
        };
        self.bus.publish(Event::LibraryMediaChanged {
            enabled,
            items,
            folders,
        });
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, StubState> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn find_download(&self, command: &str, download_id: DownloadId) -> Result<Download, CommandError> {
        self.lock_state()
            .model
            .downloads
            .iter()
            .find(|d| d.id == download_id)
            .cloned()
            .ok_or_else(|| not_found(command, "download"))
    }

    fn find_media(
        &self,
        command: &str,
        media_id: LibraryMediaId,
    ) -> Result<library_media::LibraryMediaItem, CommandError> {
        self.lock_state()
            .model
            .library_media
            .iter()
            .find(|m| m.id == media_id)
            .cloned()
            .ok_or_else(|| not_found(command, "media item"))
    }
}

fn not_found(command: &str, what: &str) -> CommandError {
    CommandError::InvalidArgument {
        command: command.to_string(),
        message: format!("{what} not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BrowserCore;

    #[test]
    fn download_lifecycle_pause_resume_tick_cancel_retry_and_clear() {
        let core = StubBackend::new();
        core.invoke(Command::DebugSampleDownload).unwrap();
        let started = core.model().downloads.last().unwrap().clone();
        assert_eq!(started.status, DownloadStatus::InProgress);
        assert!(started.simulated);

        // Tick advances progress.
        core.invoke(Command::DownloadTick).unwrap();
        let after_tick = core
            .model()
            .downloads
            .iter()
            .find(|d| d.id == started.id)
            .unwrap()
            .clone();
        assert!(after_tick.bytes_received > 0);
        assert_eq!(after_tick.status, DownloadStatus::InProgress);

        // Pause halts progress.
        core.invoke(Command::DownloadPause {
            download_id: started.id,
        })
        .unwrap();
        let paused_bytes = core
            .model()
            .downloads
            .iter()
            .find(|d| d.id == started.id)
            .unwrap()
            .bytes_received;
        core.invoke(Command::DownloadTick).unwrap();
        assert_eq!(
            core.model()
                .downloads
                .iter()
                .find(|d| d.id == started.id)
                .unwrap()
                .bytes_received,
            paused_bytes
        );

        // Resume and tick to completion.
        core.invoke(Command::DownloadResume {
            download_id: started.id,
        })
        .unwrap();
        for _ in 0..32 {
            core.invoke(Command::DownloadTick).unwrap();
        }
        let done = core
            .model()
            .downloads
            .iter()
            .find(|d| d.id == started.id)
            .unwrap()
            .clone();
        assert_eq!(done.status, DownloadStatus::Completed);
        assert_eq!(Some(done.bytes_received), done.total_bytes);

        // Open and show in folder succeed on completed download.
        core.invoke(Command::DownloadOpen {
            download_id: started.id,
        })
        .unwrap();
        core.invoke(Command::DownloadShowInFolder {
            download_id: started.id,
        })
        .unwrap();

        // Cancel and retry another download.
        core.invoke(Command::DebugSampleDownload).unwrap();
        let second = core.model().downloads.last().unwrap().id;
        core.invoke(Command::DownloadCancel { download_id: second }).unwrap();
        assert_eq!(
            core.model().downloads.iter().find(|d| d.id == second).unwrap().status,
            DownloadStatus::Cancelled
        );
        core.invoke(Command::DownloadRetry { download_id: second }).unwrap();
        assert_eq!(
            core.model().downloads.iter().find(|d| d.id == second).unwrap().status,
            DownloadStatus::InProgress
        );

        // Clear finished removes completed items but preserves in-progress ones.
        core.invoke(Command::DownloadClearFinished).unwrap();
        assert!(!core.model().downloads.iter().any(|d| d.id == started.id));
        assert!(core.model().downloads.iter().any(|d| d.id == second));

        // Remove drops one download.
        core.invoke(Command::DownloadRemove { download_id: second }).unwrap();
        assert!(!core.model().downloads.iter().any(|d| d.id == second));
    }

    #[test]
    fn library_media_opt_in_and_preview_commands() {
        let core = StubBackend::new();
        let sub = core.subscribe();
        assert!(!core.model().library_media_enabled);
        assert!(core.model().library_media.is_empty());

        core.invoke(Command::LibraryMediaSetEnabled { enabled: true }).unwrap();
        let model = core.model();
        assert!(model.library_media_enabled);
        assert!(!model.library_media.is_empty());
        let first_id = model.library_media[0].id;

        core.invoke(Command::LibraryMediaPreviewOpen { media_id: first_id })
            .unwrap();
        core.invoke(Command::LibraryMediaPreviewStep { delta: 1 }).unwrap();
        core.invoke(Command::LibraryMediaCopy { media_id: first_id }).unwrap();
        core.invoke(Command::LibraryMediaOpen { media_id: first_id }).unwrap();
        core.invoke(Command::LibraryMediaShowInFolder { media_id: first_id })
            .unwrap();
        core.invoke(Command::LibraryMediaPreviewClose).unwrap();

        let mut saw_open = false;
        let mut saw_step = false;
        let mut saw_close = false;
        let mut saw_copy = false;
        while let Ok(event) = sub.try_recv() {
            match event {
                Event::ViewRequested {
                    request: ViewRequest::LibraryMediaPreviewOpen { media_id },
                    ..
                } if media_id == first_id => saw_open = true,
                Event::ViewRequested {
                    request: ViewRequest::LibraryMediaPreviewStep { delta: 1 },
                    ..
                } => saw_step = true,
                Event::ViewRequested {
                    request: ViewRequest::LibraryMediaPreviewClose,
                    ..
                } => saw_close = true,
                Event::ClipboardCopyRequested { .. } => saw_copy = true,
                _ => {}
            }
        }
        assert!(saw_open && saw_step && saw_close && saw_copy);
    }

    #[test]
    fn apply_event_keeps_view_model_in_sync_for_downloads_and_media() {
        let core = StubBackend::new();
        let sub = core.subscribe();
        let mut view_model = core.model();

        core.invoke(Command::DebugSampleDownload).unwrap();
        let dl_id = core.model().downloads.last().unwrap().id;
        core.invoke(Command::DownloadTick).unwrap();
        core.invoke(Command::DownloadPause { download_id: dl_id }).unwrap();
        core.invoke(Command::LibraryMediaSetEnabled { enabled: true }).unwrap();

        while let Ok(event) = sub.try_recv() {
            view_model.apply_event(&event);
        }

        let core_model = core.model();
        assert_eq!(view_model.downloads, core_model.downloads);
        assert_eq!(view_model.library_media_enabled, core_model.library_media_enabled);
        assert_eq!(view_model.library_media, core_model.library_media);
        assert_eq!(view_model.library_media_folders, core_model.library_media_folders);
    }
}
