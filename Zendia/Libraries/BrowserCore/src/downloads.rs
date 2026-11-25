/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Library Downloads section logic: file classification, search and filter predicates, day grouping, human-readable
//! transfer formatting, and row context menus.

use crate::menus::ResolvedMenuItem;
use crate::model::{Download, DownloadId, DownloadStatus};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds in one day (`86_400`).
pub const SECS_PER_DAY: u64 = 86_400;

/// File-type filter groups offered by the Downloads filter panel, in Zen's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DownloadFileGroup {
    Images,
    Video,
    Audio,
    Documents,
    Archives,
    Apps,
}

impl DownloadFileGroup {
    pub const ALL: [DownloadFileGroup; 6] = [
        DownloadFileGroup::Images,
        DownloadFileGroup::Video,
        DownloadFileGroup::Audio,
        DownloadFileGroup::Documents,
        DownloadFileGroup::Archives,
        DownloadFileGroup::Apps,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Images => "images",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Documents => "documents",
            Self::Archives => "archives",
            Self::Apps => "apps",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Images => "Images",
            Self::Video => "Video",
            Self::Audio => "Audio",
            Self::Documents => "Documents",
            Self::Archives => "Archives",
            Self::Apps => "Apps",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|group| group.id() == id)
    }
}

/// Exclusive recency filter offered by the Downloads filter panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DownloadWhenFilter {
    Today,
    ThisWeek,
    ThisMonth,
}

impl DownloadWhenFilter {
    pub const ALL: [DownloadWhenFilter; 3] = [
        DownloadWhenFilter::Today,
        DownloadWhenFilter::ThisWeek,
        DownloadWhenFilter::ThisMonth,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::ThisWeek => "week",
            Self::ThisMonth => "month",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::ThisWeek => "This Week",
            Self::ThisMonth => "This Month",
        }
    }

    pub fn days(self) -> u64 {
        match self {
            Self::Today => 1,
            Self::ThisWeek => 7,
            Self::ThisMonth => 30,
        }
    }
}

/// Active search query and filter chips for the Downloads section.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DownloadFilter {
    pub query: String,
    pub file_groups: Vec<DownloadFileGroup>,
    pub when: Option<DownloadWhenFilter>,
}

impl DownloadFilter {
    pub fn is_active(&self) -> bool {
        !self.query.trim().is_empty() || !self.file_groups.is_empty() || self.when.is_some()
    }

    pub fn active_filter_count(&self) -> usize {
        self.file_groups.len() + usize::from(self.when.is_some())
    }
}

/// Downloads belonging to a single calendar day, ordered newest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadDayGroup<'a> {
    pub label: String,
    pub day_index: u64,
    pub downloads: Vec<&'a Download>,
}

/// Returns the current Unix timestamp in seconds.
pub fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(1_764_100_000)
}

/// Extracts the lowercased extension of `file_name` (without the leading dot), or an empty string if none exists.
pub fn extension_of(file_name: &str) -> String {
    let leaf = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    match leaf.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < leaf.len() => leaf[dot + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// Classifies `file_name` into one of Zen's Downloads filter groups.
pub fn file_group_of(file_name: &str) -> Option<DownloadFileGroup> {
    match extension_of(file_name).as_str() {
        "jpg" | "jpeg" | "jpe" | "png" | "gif" | "webp" | "avif" | "bmp" | "ico" | "icns" | "svg" | "heic" | "heif"
        | "jxl" | "tif" | "tiff" | "cr2" | "cr3" | "nef" | "arw" | "dng" | "raf" | "orf" | "rw2" => {
            Some(DownloadFileGroup::Images)
        }
        "mp4" | "m4v" | "mov" | "webm" | "mkv" | "avi" | "wmv" | "flv" | "mpg" | "mpeg" | "mpe" | "3gp" | "3g2"
        | "ogv" | "mts" | "m2ts" => Some(DownloadFileGroup::Video),
        "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" | "opus" | "wma" | "aiff" | "weba" => {
            Some(DownloadFileGroup::Audio)
        }
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "rtf" | "odt" | "ods" | "odp"
        | "csv" | "epub" | "pages" | "numbers" | "key" => Some(DownloadFileGroup::Documents),
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "tgz" | "zst" => Some(DownloadFileGroup::Archives),
        "dmg" | "pkg" | "exe" | "msi" | "app" | "deb" | "rpm" | "appimage" | "apk" | "jar" => {
            Some(DownloadFileGroup::Apps)
        }
        _ => None,
    }
}

/// Returns whether Skia/Freya can directly render a raster thumbnail for `file_name`.
pub fn can_draw_thumbnail(file_name: &str) -> bool {
    matches!(
        extension_of(file_name).as_str(),
        "jpg" | "jpeg" | "jpe" | "png" | "gif" | "webp" | "bmp" | "ico"
    )
}

/// Returns whether a download has a completed file present on disk.
pub fn has_file(download: &Download) -> bool {
    download.status == DownloadStatus::Completed && download.file_exists
}

/// Returns whether a download is still active or paused with partial data.
pub fn is_pending(download: &Download) -> bool {
    matches!(download.status, DownloadStatus::InProgress | DownloadStatus::Paused)
}

/// Returns whether a download can be paused.
pub fn can_pause(download: &Download) -> bool {
    download.status == DownloadStatus::InProgress
}

/// Returns whether a download can be resumed.
pub fn can_resume(download: &Download) -> bool {
    download.status == DownloadStatus::Paused
}

/// Returns whether a download can be retried.
pub fn can_retry(download: &Download) -> bool {
    matches!(
        download.status,
        DownloadStatus::Failed { .. } | DownloadStatus::Cancelled
    )
}

/// Returns whether a download can be cancelled.
pub fn can_cancel(download: &Download) -> bool {
    matches!(download.status, DownloadStatus::InProgress | DownloadStatus::Paused)
}

/// Returns whether a download is finished (completed, failed, or cancelled) and eligible for Clear Finished.
pub fn is_finished(download: &Download) -> bool {
    matches!(
        download.status,
        DownloadStatus::Completed | DownloadStatus::Failed { .. } | DownloadStatus::Cancelled
    )
}

/// Progress fraction in `0.0..=1.0`, or `None` when total size is unknown.
pub fn progress_fraction(download: &Download) -> Option<f32> {
    let total = download.total_bytes.filter(|total| *total > 0)?;
    Some((download.bytes_received as f64 / total as f64).clamp(0.0, 1.0) as f32)
}

/// Returns whether `download` matches `filter` at reference timestamp `now_unix`.
pub fn matches_filter(download: &Download, filter: &DownloadFilter, now_unix: u64) -> bool {
    if !filter.file_groups.is_empty() {
        let Some(group) = file_group_of(&download.filename) else {
            return false;
        };
        if !filter.file_groups.contains(&group) {
            return false;
        }
    }

    if let Some(when) = filter.when {
        let cutoff = now_unix.saturating_sub(when.days() * SECS_PER_DAY);
        if download.updated_at_unix < cutoff {
            return false;
        }
    }

    let query = filter.query.trim().to_ascii_lowercase();
    if !query.is_empty() {
        let name_match = download.filename.to_ascii_lowercase().contains(&query);
        let url_match = download.url.to_ascii_lowercase().contains(&query);
        if !name_match && !url_match {
            return false;
        }
    }

    true
}

/// Filters downloads, orders newest first, and groups them by calendar day relative to `now_unix`.
pub fn group_downloads_by_day<'a>(
    downloads: &'a [Download],
    filter: &DownloadFilter,
    now_unix: u64,
) -> Vec<DownloadDayGroup<'a>> {
    let mut matching: Vec<&'a Download> = downloads
        .iter()
        .filter(|download| matches_filter(download, filter, now_unix))
        .collect();
    matching.sort_by(|a, b| {
        b.updated_at_unix
            .cmp(&a.updated_at_unix)
            .then_with(|| b.id.0.cmp(&a.id.0))
    });

    let today_index = now_unix / SECS_PER_DAY;
    let mut buckets: BTreeMap<u64, Vec<&'a Download>> = BTreeMap::new();
    for download in matching {
        let day_index = if download.updated_at_unix == 0 {
            today_index
        } else {
            download.updated_at_unix / SECS_PER_DAY
        };
        buckets.entry(day_index).or_default().push(download);
    }

    buckets
        .into_iter()
        .rev()
        .map(|(day_index, items)| DownloadDayGroup {
            label: format_day_heading(day_index, today_index),
            day_index,
            downloads: items,
        })
        .collect()
}

/// Formats a day index as `"Today"`, `"Yesterday"`, or a calendar date such as `"Nov 23, 2026"`.
pub fn format_day_heading(day_index: u64, today_index: u64) -> String {
    if day_index >= today_index {
        return "Today".to_string();
    }
    if today_index - day_index == 1 {
        return "Yesterday".to_string();
    }
    let (year, month, day) = civil_from_days(day_index as i64);
    let month_name = match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        _ => "Dec",
    };
    format!("{month_name} {day}, {year}")
}

/// Converts days since 1970-01-01 to `(year, month, day)` using Howard Hinnant's civil calendar algorithm.
pub fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + i64::from(m <= 2);
    (year, m as u32, d as u32)
}

/// Formats a byte count into value and unit (`(1.2, "MB")`).
pub fn split_byte_units(bytes: u64) -> (String, &'static str) {
    const KB: f64 = 1_000.0;
    const MB: f64 = 1_000_000.0;
    const GB: f64 = 1_000_000_000.0;

    if bytes < 1_000 {
        (bytes.to_string(), "B")
    } else if (bytes as f64) < MB {
        (format_compact_decimal(bytes as f64 / KB), "KB")
    } else if (bytes as f64) < GB {
        (format_compact_decimal(bytes as f64 / MB), "MB")
    } else {
        (format_compact_decimal(bytes as f64 / GB), "GB")
    }
}

fn format_compact_decimal(value: f64) -> String {
    if value >= 100.0 {
        format!("{value:.0}")
    } else {
        let s = format!("{value:.1}");
        s.strip_suffix(".0").unwrap_or(&s).to_string()
    }
}

/// Formats a byte count such as `"2.5 MB"` or `"640 KB"`.
pub fn format_bytes(bytes: u64) -> String {
    let (value, unit) = split_byte_units(bytes);
    format!("{value} {unit}")
}

/// Formats a transfer rate such as `"3.2 MB/s"`.
pub fn format_speed(bytes_per_sec: u64) -> String {
    let (value, unit) = split_byte_units(bytes_per_sec);
    format!("{value} {unit}/s")
}

/// Formats remaining transfer time such as `"1m 14s"` or `"28s"`.
pub fn format_time_left(bytes_received: u64, total_bytes: u64, speed_bps: u64) -> Option<String> {
    if speed_bps == 0 || bytes_received >= total_bytes {
        return None;
    }
    let remaining_secs = (total_bytes - bytes_received).div_ceil(speed_bps);
    if remaining_secs < 60 {
        Some(format!("{remaining_secs}s"))
    } else if remaining_secs < 3_600 {
        let mins = remaining_secs / 60;
        let secs = remaining_secs % 60;
        if secs > 0 {
            Some(format!("{mins}m {secs}s"))
        } else {
            Some(format!("{mins}m"))
        }
    } else {
        let hours = remaining_secs / 3_600;
        let mins = (remaining_secs % 3_600) / 60;
        if mins > 0 {
            Some(format!("{hours}h {mins}m"))
        } else {
            Some(format!("{hours}h"))
        }
    }
}

/// Formats the progress segment (`"156/420 MB"` or `"640 KB/2.4 MB"`).
pub fn format_transfer_bytes(bytes_received: u64, total_bytes: Option<u64>) -> String {
    let (cur_val, cur_unit) = split_byte_units(bytes_received);
    match total_bytes.filter(|t| *t > 0) {
        Some(total) => {
            let (tot_val, tot_unit) = split_byte_units(total);
            if cur_unit == tot_unit {
                format!("{cur_val}/{tot_val} {tot_unit}")
            } else {
                format!("{cur_val} {cur_unit}/{tot_val} {tot_unit}")
            }
        }
        None => format!("{cur_val} {cur_unit}"),
    }
}

/// Strips the scheme, optional `www.` prefix, and trailing slash for hover URL display.
pub fn format_display_url(url: &str) -> String {
    let stripped = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let stripped = stripped.strip_prefix("www.").unwrap_or(stripped);
    stripped.trim_end_matches('/').to_string()
}

/// Extracts the bare host name from a download URL.
pub fn display_host(url: &str) -> String {
    let cleaned = format_display_url(url);
    cleaned.split('/').next().unwrap_or("").to_string()
}

/// Formats the status subtitle under a download's filename, matching Zen's Downloads section.
pub fn format_status_line(download: &Download, now_unix: u64) -> String {
    match &download.status {
        DownloadStatus::InProgress => {
            if download.open_when_done {
                return "Opens when finished".to_string();
            }
            let mut parts = vec![format_transfer_bytes(download.bytes_received, download.total_bytes)];
            if let Some(speed) = download.speed_bytes_per_sec.filter(|s| *s > 0) {
                parts.push(format_speed(speed));
                if let Some(total) = download.total_bytes
                    && let Some(eta) = format_time_left(download.bytes_received, total, speed)
                {
                    parts.push(eta);
                }
            }
            parts.join(" · ")
        }
        DownloadStatus::Paused => {
            let progress = format_transfer_bytes(download.bytes_received, download.total_bytes);
            format!("Paused · {progress}")
        }
        DownloadStatus::Completed => {
            if !download.file_exists {
                return "File moved or missing".to_string();
            }
            let mut parts = Vec::new();
            let size = download.total_bytes.unwrap_or(download.bytes_received);
            if size > 0 {
                parts.push(format_bytes(size));
            }
            let host = display_host(&download.url);
            if !host.is_empty() {
                parts.push(host);
            }
            if download.updated_at_unix > 0 {
                let today_index = now_unix / SECS_PER_DAY;
                let day_index = download.updated_at_unix / SECS_PER_DAY;
                parts.push(format_day_heading(day_index, today_index));
            }
            if parts.is_empty() {
                "Completed".to_string()
            } else {
                parts.join(" · ")
            }
        }
        DownloadStatus::Failed { reason } => {
            if reason.trim().is_empty() {
                "Failed".to_string()
            } else {
                format!("Failed · {}", reason.trim())
            }
        }
        DownloadStatus::Cancelled => "Canceled".to_string(),
    }
}

/// Builds the context menu entries for a download row.
pub fn build_download_context_menu(download: &Download) -> Vec<ResolvedMenuItem> {
    let id = download.id.0.to_string();
    let mut items = Vec::new();

    let action_item = |item_id: &str, label: String, cmd: &str, icon: &str, enabled: bool, with_id: bool| {
        let mut item = ResolvedMenuItem::action(item_id, label, cmd).with_enabled(enabled);
        item.icon = Some(icon.to_string());
        if with_id {
            item = item.with_arg("download_id", id.clone());
        }
        item
    };

    if can_pause(download) {
        items.push(action_item(
            "download.ctx.pause",
            "Pause".to_string(),
            "download.pause",
            "media-pause",
            true,
            true,
        ));
    }
    if can_resume(download) {
        items.push(action_item(
            "download.ctx.resume",
            "Resume".to_string(),
            "download.resume",
            "media-play",
            true,
            true,
        ));
    }
    if can_retry(download) {
        items.push(action_item(
            "download.ctx.retry",
            "Retry Download".to_string(),
            "download.retry",
            "reload",
            true,
            true,
        ));
    }
    if can_cancel(download) {
        items.push(action_item(
            "download.ctx.cancel",
            "Cancel Download".to_string(),
            "download.cancel",
            "close",
            true,
            true,
        ));
    }

    let file_available = has_file(download);
    items.push(action_item(
        "download.ctx.open",
        format!("Open {}", download.filename),
        "download.open",
        "open",
        file_available,
        true,
    ));
    items.push(action_item(
        "download.ctx.show",
        "Show in Folder".to_string(),
        "download.show_in_folder",
        "folder",
        file_available,
        true,
    ));
    items.push(ResolvedMenuItem::separator("download.ctx.sep.1"));
    items.push(action_item(
        "download.ctx.copy_link",
        "Copy Download Link".to_string(),
        "download.copy_link",
        "link",
        true,
        true,
    ));
    items.push(ResolvedMenuItem::separator("download.ctx.sep.2"));
    items.push(action_item(
        "download.ctx.remove",
        "Remove from List".to_string(),
        "download.remove",
        "trash",
        true,
        true,
    ));
    items.push(action_item(
        "download.ctx.clear_finished",
        "Clear Finished Downloads".to_string(),
        "download.clear_finished",
        "edit-delete",
        true,
        false,
    ));

    items
}

/// Builds a mixed-state download list spanning multiple days for the gallery fixtures.
pub fn sample_mixed_downloads(now_unix: u64, sample_image_path: Option<String>) -> Vec<Download> {
    let today = now_unix;
    let yesterday = now_unix.saturating_sub(SECS_PER_DAY + 3_600);
    let four_days_ago = now_unix.saturating_sub(4 * SECS_PER_DAY + 1_800);

    vec![
        Download {
            id: DownloadId(101),
            filename: "zendia-linux-x86_64.tar.gz".to_string(),
            url: "https://releases.zendia.org/nightly/zendia-linux-x86_64.tar.gz".to_string(),
            bytes_received: 54_800_000,
            total_bytes: Some(88_400_000),
            status: DownloadStatus::InProgress,
            target_path: Some("/home/user/Downloads/zendia-linux-x86_64.tar.gz".to_string()),
            speed_bytes_per_sec: Some(4_200_000),
            updated_at_unix: today,
            file_exists: true,
            open_when_done: false,
            simulated: false,
        },
        Download {
            id: DownloadId(102),
            filename: "design-assets-v3.zip".to_string(),
            url: "https://cdn.zendia.org/kits/design-assets-v3.zip".to_string(),
            bytes_received: 18_400_000,
            total_bytes: Some(48_000_000),
            status: DownloadStatus::Paused,
            target_path: Some("/home/user/Downloads/design-assets-v3.zip".to_string()),
            speed_bytes_per_sec: None,
            updated_at_unix: today.saturating_sub(300),
            file_exists: true,
            open_when_done: false,
            simulated: false,
        },
        Download {
            id: DownloadId(103),
            filename: "aurora-workspace.png".to_string(),
            url: "https://media.zendia.org/wallpapers/aurora-workspace.png".to_string(),
            bytes_received: 1_840_000,
            total_bytes: Some(1_840_000),
            status: DownloadStatus::Completed,
            target_path: sample_image_path,
            speed_bytes_per_sec: None,
            updated_at_unix: today.saturating_sub(1_200),
            file_exists: true,
            open_when_done: false,
            simulated: false,
        },
        Download {
            id: DownloadId(104),
            filename: "ladybird-architecture.pdf".to_string(),
            url: "https://ladybird.org/docs/architecture.pdf".to_string(),
            bytes_received: 2_450_000,
            total_bytes: Some(2_450_000),
            status: DownloadStatus::Completed,
            target_path: Some("/home/user/Downloads/ladybird-architecture.pdf".to_string()),
            speed_bytes_per_sec: None,
            updated_at_unix: today.saturating_sub(5_400),
            file_exists: true,
            open_when_done: false,
            simulated: false,
        },
        Download {
            id: DownloadId(105),
            filename: "nightly-build-2026.appimage".to_string(),
            url: "https://builds.zendia.org/nightly-build-2026.appimage".to_string(),
            bytes_received: 12_100_000,
            total_bytes: Some(96_000_000),
            status: DownloadStatus::Failed {
                reason: "Network error".to_string(),
            },
            target_path: Some("/home/user/Downloads/nightly-build-2026.appimage".to_string()),
            speed_bytes_per_sec: None,
            updated_at_unix: yesterday,
            file_exists: false,
            open_when_done: false,
            simulated: false,
        },
        Download {
            id: DownloadId(106),
            filename: "raw-capture-04.mp4".to_string(),
            url: "https://clips.zendia.org/raw-capture-04.mp4".to_string(),
            bytes_received: 6_200_000,
            total_bytes: Some(64_000_000),
            status: DownloadStatus::Cancelled,
            target_path: Some("/home/user/Downloads/raw-capture-04.mp4".to_string()),
            speed_bytes_per_sec: None,
            updated_at_unix: yesterday.saturating_sub(1_800),
            file_exists: false,
            open_when_done: false,
            simulated: false,
        },
        Download {
            id: DownloadId(107),
            filename: "release-checklist.md".to_string(),
            url: "https://docs.zendia.org/release-checklist.md".to_string(),
            bytes_received: 18_400,
            total_bytes: Some(18_400),
            status: DownloadStatus::Completed,
            target_path: Some("/home/user/Downloads/release-checklist.md".to_string()),
            speed_bytes_per_sec: None,
            updated_at_unix: four_days_ago,
            file_exists: false,
            open_when_done: false,
            simulated: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_now() -> u64 {
        // 2026-11-26 12:00:00 UTC
        1_795_694_400
    }

    #[test]
    fn classifies_file_groups_and_thumbnails() {
        assert_eq!(file_group_of("photo.PNG"), Some(DownloadFileGroup::Images));
        assert_eq!(file_group_of("clip.mkv"), Some(DownloadFileGroup::Video));
        assert_eq!(file_group_of("track.ogg"), Some(DownloadFileGroup::Audio));
        assert_eq!(file_group_of("spec.pdf"), Some(DownloadFileGroup::Documents));
        assert_eq!(file_group_of("archive.tar.gz"), Some(DownloadFileGroup::Archives));
        assert_eq!(file_group_of("installer.AppImage"), Some(DownloadFileGroup::Apps));
        assert_eq!(file_group_of("README"), None);

        assert!(can_draw_thumbnail("hero.png"));
        assert!(can_draw_thumbnail("photo.JPEG"));
        assert!(!can_draw_thumbnail("vector.svg"));
        assert!(!can_draw_thumbnail("raw.dng"));
    }

    #[test]
    fn groups_newest_first_by_day_and_filters_by_query_type_and_when() {
        let now = sample_now();
        let list = sample_mixed_downloads(now, None);
        let groups = group_downloads_by_day(&list, &DownloadFilter::default(), now);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].label, "Today");
        assert_eq!(groups[0].downloads.len(), 4);
        assert_eq!(groups[0].downloads[0].id, DownloadId(101));
        assert_eq!(groups[1].label, "Yesterday");
        assert_eq!(groups[1].downloads.len(), 2);
        assert_eq!(groups[2].downloads.len(), 1);

        let archive_filter = DownloadFilter {
            query: String::new(),
            file_groups: vec![DownloadFileGroup::Archives],
            when: None,
        };
        let archives = group_downloads_by_day(&list, &archive_filter, now);
        assert_eq!(archives.len(), 1);
        assert_eq!(archives[0].downloads.len(), 2);

        let url_search = DownloadFilter {
            query: "ladybird.org".to_string(),
            file_groups: Vec::new(),
            when: None,
        };
        let found = group_downloads_by_day(&list, &url_search, now);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].downloads[0].filename, "ladybird-architecture.pdf");

        let today_only = DownloadFilter {
            query: String::new(),
            file_groups: Vec::new(),
            when: Some(DownloadWhenFilter::Today),
        };
        let recent = group_downloads_by_day(&list, &today_only, now);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].label, "Today");
    }

    #[test]
    fn formats_status_lines_and_transfer_units() {
        let now = sample_now();
        let list = sample_mixed_downloads(now, None);

        let in_progress = format_status_line(&list[0], now);
        assert!(in_progress.contains("54.8/88.4 MB"));
        assert!(in_progress.contains("4.2 MB/s"));
        assert!(in_progress.contains("8s"));

        let paused = format_status_line(&list[1], now);
        assert_eq!(paused, "Paused · 18.4/48 MB");

        let completed = format_status_line(&list[3], now);
        assert_eq!(completed, "2.5 MB · ladybird.org · Today");

        let failed = format_status_line(&list[4], now);
        assert_eq!(failed, "Failed · Network error");

        let cancelled = format_status_line(&list[5], now);
        assert_eq!(cancelled, "Canceled");

        let missing = format_status_line(&list[6], now);
        assert_eq!(missing, "File moved or missing");
    }

    #[test]
    fn context_menu_adapts_to_download_state() {
        let now = sample_now();
        let list = sample_mixed_downloads(now, None);

        let active_menu = build_download_context_menu(&list[0]);
        assert!(active_menu.iter().any(|i| i.command == "download.pause"));
        assert!(active_menu.iter().any(|i| i.command == "download.cancel"));
        assert!(!active_menu.iter().any(|i| i.command == "download.resume"));
        assert!(
            !active_menu
                .iter()
                .find(|i| i.command == "download.open")
                .unwrap()
                .enabled
        );

        let paused_menu = build_download_context_menu(&list[1]);
        assert!(paused_menu.iter().any(|i| i.command == "download.resume"));
        assert!(!paused_menu.iter().any(|i| i.command == "download.pause"));

        let done_menu = build_download_context_menu(&list[3]);
        assert!(done_menu.iter().find(|i| i.command == "download.open").unwrap().enabled);
        assert!(
            done_menu
                .iter()
                .find(|i| i.command == "download.show_in_folder")
                .unwrap()
                .enabled
        );

        let failed_menu = build_download_context_menu(&list[4]);
        assert!(failed_menu.iter().any(|i| i.command == "download.retry"));
    }
}
