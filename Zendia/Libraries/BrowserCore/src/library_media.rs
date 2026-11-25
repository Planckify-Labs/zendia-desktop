/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Library Media section logic: folder scanning matching Zen's depth/item bounds, extraction of image and video
//! downloads, header-based image dimension probing, grid ordering, preview formatting, and deterministic sample PNG
//! fixture generation.

use crate::downloads::{SECS_PER_DAY, can_draw_thumbnail, extension_of, has_file};
use crate::menus::ResolvedMenuItem;
use crate::model::{Download, LibraryMediaId};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Maximum folder recursion depth when scanning configured media roots.
pub const MAX_SCAN_DEPTH: usize = 2;
/// Maximum number of directories visited in one scan.
pub const MAX_SCAN_FOLDERS: usize = 60;
/// Maximum number of media items retained after sorting newest first.
pub const MAX_SCAN_ITEMS: usize = 300;

/// Extensions collected during a Media folder scan (images and videos).
pub const MEDIA_EXTENSIONS: &[&str] = &[
    // Images
    "jpg", "jpeg", "jpe", "png", "gif", "webp", "avif", "bmp", "ico", "icns", "svg", "heic", "heif", "jxl", "tif",
    "tiff", "cr2", "cr3", "nef", "arw", "dng", "raf", "orf", "rw2", // Video
    "mp4", "m4v", "mov", "webm", "mkv", "avi", "wmv", "flv", "mpg", "mpeg", "mpe", "3gp", "3g2", "ogv", "mts", "m2ts",
];

/// How the Media grid and preview present an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LibraryMediaKind {
    /// Raster image that Freya/Skia can draw directly as a thumbnail and full preview.
    Image,
    /// Video clip shown with a play badge in the grid and playback preview chrome.
    Video,
    /// Image format without a direct thumbnail decoder (e.g. HEIC, RAW, TIFF, SVG), rendered as a file icon.
    File,
}

/// One image or video item surfaced in the library's Media section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryMediaItem {
    pub id: LibraryMediaId,
    pub name: String,
    pub path: String,
    pub folder: String,
    pub extension: String,
    pub kind: LibraryMediaKind,
    pub size_bytes: u64,
    pub dimensions: Option<(u32, u32)>,
    pub modified_at_unix: u64,
    /// Optional local poster frame image for video tiles and preview.
    pub thumbnail_path: Option<String>,
    /// Optional video duration in seconds.
    pub duration_secs: Option<u32>,
}

/// Classifies `file_name` for the Media grid, returning `None` for non-media files.
pub fn media_kind_of(file_name: &str) -> Option<LibraryMediaKind> {
    let ext = extension_of(file_name);
    if !MEDIA_EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }
    if matches!(
        ext.as_str(),
        "mp4"
            | "m4v"
            | "mov"
            | "webm"
            | "mkv"
            | "avi"
            | "wmv"
            | "flv"
            | "mpg"
            | "mpeg"
            | "mpe"
            | "3gp"
            | "3g2"
            | "ogv"
            | "mts"
            | "m2ts"
    ) {
        return Some(LibraryMediaKind::Video);
    }
    if can_draw_thumbnail(file_name) {
        Some(LibraryMediaKind::Image)
    } else {
        Some(LibraryMediaKind::File)
    }
}

/// Reads `(width, height)` from the header of a local PNG, GIF, BMP, or JPEG file without decoding pixel data.
pub fn read_image_dimensions(path: &Path) -> Option<(u32, u32)> {
    let mut file = File::open(path).ok()?;
    let mut header = [0u8; 32];
    let n = file.read(&mut header).ok()?;
    if n < 10 {
        return None;
    }
    // PNG: 8-byte signature + 4-byte length + "IHDR" + 4-byte BE width + 4-byte BE height.
    if n >= 24 && &header[0..8] == b"\x89PNG\r\n\x1a\n" && &header[12..16] == b"IHDR" {
        let w = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);
        let h = u32::from_be_bytes([header[20], header[21], header[22], header[23]]);
        return (w > 0 && h > 0).then_some((w, h));
    }
    // GIF: "GIF87a" / "GIF89a" + 2-byte LE width + 2-byte LE height.
    if (&header[0..6] == b"GIF87a" || &header[0..6] == b"GIF89a") && n >= 10 {
        let w = u16::from_le_bytes([header[6], header[7]]) as u32;
        let h = u16::from_le_bytes([header[8], header[9]]) as u32;
        return (w > 0 && h > 0).then_some((w, h));
    }
    // BMP: "BM" + ... + DIB header width/height at offset 18..26 (i32 LE).
    if &header[0..2] == b"BM" && n >= 26 {
        let w = i32::from_le_bytes([header[18], header[19], header[20], header[21]]).unsigned_abs();
        let h = i32::from_le_bytes([header[22], header[23], header[24], header[25]]).unsigned_abs();
        return (w > 0 && h > 0).then_some((w, h));
    }
    // JPEG: SOI (FF D8), scan for SOF0/SOF2 marker.
    if header[0] == 0xFF && header[1] == 0xD8 {
        let mut bytes = Vec::new();
        let mut file = File::open(path).ok()?;
        let _ = file.by_ref().take(65_536).read_to_end(&mut bytes);
        let mut i = 2usize;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xFF {
                i += 1;
                continue;
            }
            while i < bytes.len() && bytes[i] == 0xFF {
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }
            let marker = bytes[i];
            i += 1;
            if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
                if i + 7 <= bytes.len() {
                    let h = u16::from_be_bytes([bytes[i + 3], bytes[i + 4]]) as u32;
                    let w = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                    return (w > 0 && h > 0).then_some((w, h));
                }
                break;
            }
            if i + 2 > bytes.len() {
                break;
            }
            let seg_len = u16::from_be_bytes([bytes[i], bytes[i + 1]]) as usize;
            if seg_len < 2 {
                break;
            }
            i += seg_len;
        }
    }
    None
}

/// Scans `roots` for media files up to `max_depth`, `max_folders`, and `max_items`, returning items newest first.
pub fn scan_media_folders(
    roots: &[PathBuf],
    max_depth: usize,
    max_folders: usize,
    max_items: usize,
) -> Vec<LibraryMediaItem> {
    let mut found = Vec::new();
    let mut folders_left = max_folders;

    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let folder_label = root
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| root.display().to_string());
        walk_folder(root, &folder_label, 0, max_depth, &mut folders_left, &mut found);
    }

    found.sort_by(|a, b| {
        b.modified_at_unix
            .cmp(&a.modified_at_unix)
            .then_with(|| a.name.cmp(&b.name))
    });
    found.truncate(max_items);
    for (idx, item) in found.iter_mut().enumerate() {
        item.id = LibraryMediaId((idx + 1) as u64);
    }
    found
}

fn walk_folder(
    dir: &Path,
    root_label: &str,
    depth: usize,
    max_depth: usize,
    folders_left: &mut usize,
    found: &mut Vec<LibraryMediaItem>,
) {
    if depth > max_depth || *folders_left == 0 {
        return;
    }
    *folders_left -= 1;

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.is_empty() || name.starts_with('.') {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            // Skip bundle directories with extensions (e.g. `.photoslibrary`).
            if !name.contains('.') {
                subdirs.push(path);
            }
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let Some(kind) = media_kind_of(&name) else {
            continue;
        };
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let modified_at_unix = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let dimensions = if kind == LibraryMediaKind::Image {
            read_image_dimensions(&path)
        } else {
            None
        };
        let path_str = path.to_string_lossy().into_owned();
        found.push(LibraryMediaItem {
            id: LibraryMediaId(0),
            extension: extension_of(&name),
            name,
            path: path_str,
            folder: root_label.to_string(),
            kind,
            size_bytes: meta.len(),
            dimensions,
            modified_at_unix,
            thumbnail_path: None,
            duration_secs: None,
        });
    }

    for subdir in subdirs {
        walk_folder(&subdir, root_label, depth + 1, max_depth, folders_left, found);
    }
}

/// Extracts completed image and video downloads into media items.
pub fn media_from_downloads(downloads: &[Download]) -> Vec<LibraryMediaItem> {
    downloads
        .iter()
        .filter(|download| has_file(download))
        .filter_map(|download| {
            let kind = media_kind_of(&download.filename)?;
            let path = download.target_path.clone().unwrap_or_default();
            let dimensions = if kind == LibraryMediaKind::Image && !path.is_empty() {
                read_image_dimensions(Path::new(&path))
            } else {
                None
            };
            Some(LibraryMediaItem {
                id: LibraryMediaId(10_000 + download.id.0),
                name: download.filename.clone(),
                path,
                folder: "Downloads".to_string(),
                extension: extension_of(&download.filename),
                kind,
                size_bytes: download.total_bytes.unwrap_or(download.bytes_received),
                dimensions,
                modified_at_unix: download.updated_at_unix,
                thumbnail_path: None,
                duration_secs: None,
            })
        })
        .collect()
}

/// Merges folder-scanned media with media from completed downloads, deduplicating by path/name and sorting newest first.
pub fn merge_media_items(
    scanned: Vec<LibraryMediaItem>,
    from_downloads: Vec<LibraryMediaItem>,
) -> Vec<LibraryMediaItem> {
    let mut seen = HashSet::new();
    let mut combined = Vec::with_capacity(scanned.len() + from_downloads.len());

    for item in scanned.into_iter().chain(from_downloads) {
        let key = if !item.path.is_empty() {
            item.path.clone()
        } else {
            item.name.to_ascii_lowercase()
        };
        if seen.insert(key) {
            combined.push(item);
        }
    }

    combined.sort_by(|a, b| {
        b.modified_at_unix
            .cmp(&a.modified_at_unix)
            .then_with(|| b.id.0.cmp(&a.id.0))
    });
    combined.truncate(MAX_SCAN_ITEMS);
    combined
}

/// Filters media items by search query (case-insensitive filename match) and active folder filter chips.
pub fn filter_media_items<'a>(
    items: &'a [LibraryMediaItem],
    query: &str,
    active_folders: &[String],
) -> Vec<&'a LibraryMediaItem> {
    let q = query.trim().to_ascii_lowercase();
    items
        .iter()
        .filter(|item| {
            let folder_ok = active_folders.is_empty() || active_folders.iter().any(|f| f == &item.folder);
            let query_ok = q.is_empty() || item.name.to_ascii_lowercase().contains(&q);
            folder_ok && query_ok
        })
        .collect()
}

/// Steps from `current` by `delta` (`-1` or `+1`) within `items`, clamping to bounds (matching Zen's preview).
pub fn step_media_id(items: &[LibraryMediaItem], current: LibraryMediaId, delta: i32) -> Option<LibraryMediaId> {
    let index = items.iter().position(|item| item.id == current)?;
    let target = index as isize + delta as isize;
    if target < 0 || target as usize >= items.len() {
        return Some(current);
    }
    Some(items[target as usize].id)
}

/// Formats a media item's relative date matching `ZenLibraryMediaPreview.#relativeDate`.
pub fn format_relative_date(modified_at_unix: u64, now_unix: u64) -> String {
    if modified_at_unix == 0 {
        return "Unknown date".to_string();
    }
    if modified_at_unix >= now_unix {
        return "Today".to_string();
    }
    let days = (now_unix - modified_at_unix) / SECS_PER_DAY;
    match days {
        0 => "Today".to_string(),
        1 => "Yesterday".to_string(),
        2..=6 => format!("{days} days ago"),
        7..=13 => "1 wk. ago".to_string(),
        _ => format!("{} wk. ago", days / 7),
    }
}

/// Formats optional pixel dimensions as `"1280 × 800"`.
pub fn format_dimensions(dimensions: Option<(u32, u32)>) -> Option<String> {
    dimensions.map(|(w, h)| format!("{w} × {h}"))
}

/// Formats a video duration in seconds as `"0:42"` or `"1:04:12"`.
pub fn format_duration(secs: u32) -> String {
    let hours = secs / 3_600;
    let mins = (secs % 3_600) / 60;
    let rem = secs % 60;
    if hours > 0 {
        format!("{hours}:{mins:02}:{rem:02}")
    } else {
        format!("{mins}:{rem:02}")
    }
}

/// Returns the system app label used in the preview's `"Open in ..."` button.
pub fn default_opener_name(kind: LibraryMediaKind) -> &'static str {
    match kind {
        LibraryMediaKind::Image => "Image Viewer",
        LibraryMediaKind::Video => "Video Player",
        LibraryMediaKind::File => "System Viewer",
    }
}

/// Builds the context menu entries for a media tile or preview stage, matching `ZenLibraryMediaPreview`.
pub fn build_media_context_menu(item: &LibraryMediaItem) -> Vec<ResolvedMenuItem> {
    let id = item.id.0.to_string();
    let entry = |entry_id: &str, label: &str, cmd: &str, icon: &str| {
        let mut item = ResolvedMenuItem::action(entry_id, label, cmd).with_arg("media_id", id.clone());
        item.icon = Some(icon.to_string());
        item
    };
    vec![
        entry("media.ctx.open", "Open", "library.media.open", "open"),
        entry(
            "media.ctx.show",
            "Show in Folder",
            "library.media.show_in_folder",
            "folder",
        ),
        ResolvedMenuItem::separator("media.ctx.sep.1"),
        entry("media.ctx.copy", "Copy", "library.media.copy", "copy"),
    ]
}

/// Materializes small, crisp sample PNG files in a deterministic temp directory and returns sample [`LibraryMediaItem`]s
/// (4 local PNG images + 1 video with a local poster PNG) along with their folder names.
pub fn materialize_sample_media(now_unix: u64) -> (Vec<LibraryMediaItem>, Vec<String>) {
    let base_dir = std::env::temp_dir().join("zendia-library-media-fixtures");
    let _ = fs::create_dir_all(&base_dir);

    let aurora_path = ensure_fixture_png(&base_dir, "aurora-workspace.png", 320, 220, SampleArt::Aurora);
    let prism_path = ensure_fixture_png(&base_dir, "tokyonight-prism.png", 260, 260, SampleArt::Prism);
    let sunset_path = ensure_fixture_png(&base_dir, "dusk-horizon.png", 320, 200, SampleArt::Sunset);
    let portrait_path = ensure_fixture_png(&base_dir, "release-poster.png", 220, 300, SampleArt::Portrait);
    let video_poster_path = ensure_fixture_png(&base_dir, "walkthrough-poster.png", 320, 180, SampleArt::VideoFrame);

    let items = vec![
        LibraryMediaItem {
            id: LibraryMediaId(1),
            name: "aurora-workspace.png".to_string(),
            path: aurora_path.to_string_lossy().into_owned(),
            folder: "Downloads".to_string(),
            extension: "png".to_string(),
            kind: LibraryMediaKind::Image,
            size_bytes: fs::metadata(&aurora_path).map(|m| m.len()).unwrap_or(184_320),
            dimensions: Some((320, 220)),
            modified_at_unix: now_unix.saturating_sub(600),
            thumbnail_path: None,
            duration_secs: None,
        },
        LibraryMediaItem {
            id: LibraryMediaId(2),
            name: "demo-walkthrough.mp4".to_string(),
            path: base_dir.join("demo-walkthrough.mp4").to_string_lossy().into_owned(),
            folder: "Videos".to_string(),
            extension: "mp4".to_string(),
            kind: LibraryMediaKind::Video,
            size_bytes: 14_600_000,
            dimensions: Some((1920, 1080)),
            modified_at_unix: now_unix.saturating_sub(3_600),
            thumbnail_path: Some(video_poster_path.to_string_lossy().into_owned()),
            duration_secs: Some(42),
        },
        LibraryMediaItem {
            id: LibraryMediaId(3),
            name: "tokyonight-prism.png".to_string(),
            path: prism_path.to_string_lossy().into_owned(),
            folder: "Pictures".to_string(),
            extension: "png".to_string(),
            kind: LibraryMediaKind::Image,
            size_bytes: fs::metadata(&prism_path).map(|m| m.len()).unwrap_or(204_800),
            dimensions: Some((260, 260)),
            modified_at_unix: now_unix.saturating_sub(SECS_PER_DAY + 900),
            thumbnail_path: None,
            duration_secs: None,
        },
        LibraryMediaItem {
            id: LibraryMediaId(4),
            name: "dusk-horizon.png".to_string(),
            path: sunset_path.to_string_lossy().into_owned(),
            folder: "Pictures".to_string(),
            extension: "png".to_string(),
            kind: LibraryMediaKind::Image,
            size_bytes: fs::metadata(&sunset_path).map(|m| m.len()).unwrap_or(196_000),
            dimensions: Some((320, 200)),
            modified_at_unix: now_unix.saturating_sub(3 * SECS_PER_DAY),
            thumbnail_path: None,
            duration_secs: None,
        },
        LibraryMediaItem {
            id: LibraryMediaId(5),
            name: "release-poster.png".to_string(),
            path: portrait_path.to_string_lossy().into_owned(),
            folder: "Downloads".to_string(),
            extension: "png".to_string(),
            kind: LibraryMediaKind::Image,
            size_bytes: fs::metadata(&portrait_path).map(|m| m.len()).unwrap_or(198_000),
            dimensions: Some((220, 300)),
            modified_at_unix: now_unix.saturating_sub(8 * SECS_PER_DAY),
            thumbnail_path: None,
            duration_secs: None,
        },
    ];

    let folders = vec!["Downloads".to_string(), "Pictures".to_string(), "Videos".to_string()];
    (items, folders)
}

/// Returns the local path to the materialized `aurora-workspace.png` fixture image for download row thumbnails.
pub fn sample_download_thumbnail_path() -> String {
    let base_dir = std::env::temp_dir().join("zendia-library-media-fixtures");
    let _ = fs::create_dir_all(&base_dir);
    ensure_fixture_png(&base_dir, "aurora-workspace.png", 320, 220, SampleArt::Aurora)
        .to_string_lossy()
        .into_owned()
}

#[derive(Clone, Copy)]
enum SampleArt {
    Aurora,
    Prism,
    Sunset,
    Portrait,
    VideoFrame,
}

fn ensure_fixture_png(dir: &Path, filename: &str, width: u32, height: u32, art: SampleArt) -> PathBuf {
    let path = dir.join(filename);
    if path.is_file() && read_image_dimensions(&path) == Some((width, height)) {
        return path;
    }
    let pixels = render_sample_art(width, height, art);
    let png_bytes = encode_rgb_png(width, height, &pixels);
    let _ = fs::write(&path, png_bytes);
    path
}

fn fast_sin(mut x: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    x = (x + PI).rem_euclid(TAU) - PI;
    let b = 4.0 / PI;
    let c = -4.0 / (PI * PI);
    let y = b * x + c * x * x.abs();
    0.225 * (y * y.abs() - y) + y
}

fn fast_cos(x: f32) -> f32 {
    fast_sin(x + std::f32::consts::FRAC_PI_2)
}

fn render_sample_art(width: u32, height: u32, art: SampleArt) -> Vec<u8> {
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    let wf = width.max(1) as f32;
    let hf = height.max(1) as f32;
    for y in 0..height {
        let ny = y as f32 / hf;
        for x in 0..width {
            let nx = x as f32 / wf;
            let (r, g, b) = match art {
                SampleArt::Aurora => {
                    // Deep Tokyonight sky with teal-violet aurora ribbon and mountain silhouette.
                    let wave = fast_sin(nx * 4.5) * 0.12 + 0.36;
                    let glow = (1.0 - ((ny - wave).abs() * 4.2)).clamp(0.0, 1.0);
                    let mountain = 0.72 + fast_sin(nx * 6.0) * 0.06 + fast_cos(nx * 13.0) * 0.03;
                    if ny > mountain {
                        (18.0 + ny * 12.0, 22.0 + ny * 14.0, 38.0 + ny * 16.0)
                    } else {
                        let base_r = 24.0 + ny * 26.0 + glow * (60.0 + nx * 100.0);
                        let base_g = 32.0 + ny * 38.0 + glow * (175.0 - nx * 40.0);
                        let base_b = 64.0 + ny * 54.0 + glow * 145.0;
                        (base_r, base_g, base_b)
                    }
                }
                SampleArt::Prism => {
                    // Geometric layered diamond rings in Tokyonight purple, blue, pink, and teal.
                    let dx = (nx - 0.5).abs();
                    let dy = (ny - 0.5).abs();
                    let d = dx + dy;
                    let ring = ((d * 8.0) as i32) % 4;
                    match ring {
                        0 => (122.0 + nx * 50.0, 162.0 + ny * 40.0, 247.0),
                        1 => (192.0, 153.0 + nx * 30.0, 255.0),
                        2 => (79.0 + ny * 40.0, 214.0, 190.0),
                        _ => (45.0 + nx * 40.0, 52.0 + ny * 30.0, 86.0),
                    }
                }
                SampleArt::Sunset => {
                    // Warm dusk gradient over dark coastal hills and glowing sun disc.
                    let sx = (nx - 0.68) * (wf / hf);
                    let sy = ny - 0.44;
                    let sun = (sx * sx + sy * sy).sqrt();
                    let hill = 0.66 + fast_sin(nx * 5.0 + 0.8) * 0.05;
                    if ny > hill {
                        (30.0 + (1.0 - ny) * 28.0, 26.0 + (1.0 - ny) * 22.0, 48.0 + ny * 18.0)
                    } else if sun < 0.14 {
                        (255.0, 212.0, 134.0)
                    } else {
                        let r = 56.0 + ny * 195.0;
                        let g = 46.0 + ny * 110.0;
                        let b = 104.0 + (1.0 - ny) * 90.0;
                        (r, g, b)
                    }
                }
                SampleArt::Portrait => {
                    // Vertical editorial poster with curved teal & magenta bands.
                    let band = fast_sin(nx * 3.1 + ny * 2.4);
                    let circle = ((nx - 0.5).powi(2) + ((ny - 0.38) * (hf / wf)).powi(2)).sqrt();
                    if circle < 0.24 {
                        (247.0, 118.0 + ny * 80.0, 142.0 + nx * 60.0)
                    } else if band > 0.15 {
                        (65.0 + ny * 45.0, 166.0 + nx * 30.0, 181.0 + ny * 35.0)
                    } else {
                        (31.0 + nx * 35.0, 35.0 + ny * 42.0, 58.0 + ny * 52.0)
                    }
                }
                SampleArt::VideoFrame => {
                    // Widescreen cinema frame with subtle dark letterbox feel and glowing blue/amber studio stage.
                    let in_frame = ny > 0.08 && ny < 0.92;
                    if !in_frame {
                        (16.0, 18.0, 26.0)
                    } else {
                        let spot = 1.0 - (((nx - 0.5) * 1.4).powi(2) + ((ny - 0.45) * 1.8).powi(2)).sqrt();
                        let spot = spot.clamp(0.0, 1.0);
                        let r = 28.0 + spot * 92.0 + nx * 42.0;
                        let g = 42.0 + spot * 116.0 + ny * 28.0;
                        let b = 78.0 + spot * 145.0;
                        (r, g, b)
                    }
                }
            };
            rgb.push(r.clamp(0.0, 255.0) as u8);
            rgb.push(g.clamp(0.0, 255.0) as u8);
            rgb.push(b.clamp(0.0, 255.0) as u8);
        }
    }
    rgb
}

/// Encodes an 8-bit RGB image into a valid PNG byte stream using uncompressed RFC 1950/1951 DEFLATE blocks.
pub fn encode_rgb_png(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    let row_len = (width as usize) * 3;
    let mut raw = Vec::with_capacity((row_len + 1) * (height as usize));
    for y in 0..(height as usize) {
        raw.push(0u8); // Filter type 0 (None)
        let start = y * row_len;
        raw.extend_from_slice(&rgb[start..start + row_len]);
    }

    // Wrap raw scanlines in a valid zlib stream using uncompressed DEFLATE blocks (max 65535 bytes each).
    let mut zlib = Vec::with_capacity(raw.len() + 64);
    zlib.push(0x78); // CMF: deflate, 32K window
    zlib.push(0x01); // FLG: no dict, check bits (0x7801 % 31 == 0)

    let mut offset = 0usize;
    while offset < raw.len() {
        let chunk_len = (raw.len() - offset).min(65_535);
        let is_final = offset + chunk_len >= raw.len();
        zlib.push(u8::from(is_final));
        let len = chunk_len as u16;
        let nlen = !len;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&nlen.to_le_bytes());
        zlib.extend_from_slice(&raw[offset..offset + chunk_len]);
        offset += chunk_len;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = Vec::with_capacity(zlib.len() + 64);
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");

    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&width.to_be_bytes());
    ihdr[4..8].copy_from_slice(&height.to_be_bytes());
    ihdr[8] = 8; // bit depth
    ihdr[9] = 2; // color type: truecolor RGB
    ihdr[10] = 0; // compression
    ihdr[11] = 0; // filter
    ihdr[12] = 0; // interlace
    push_png_chunk(&mut png, b"IHDR", &ihdr);
    push_png_chunk(&mut png, b"IDAT", &zlib);
    push_png_chunk(&mut png, b"IEND", &[]);
    png
}

fn push_png_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(chunk_type);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let mut s1 = 1u32;
    let mut s2 = 0u32;
    for &b in data {
        s1 = (s1 + b as u32) % MOD;
        s2 = (s2 + s1) % MOD;
    }
    (s2 << 16) | s1
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DownloadId, DownloadStatus};

    #[test]
    fn classifies_media_kinds_like_zen() {
        assert_eq!(media_kind_of("photo.png"), Some(LibraryMediaKind::Image));
        assert_eq!(media_kind_of("clip.mp4"), Some(LibraryMediaKind::Video));
        assert_eq!(media_kind_of("clip.mkv"), Some(LibraryMediaKind::Video));
        assert_eq!(media_kind_of("diagram.svg"), Some(LibraryMediaKind::File));
        assert_eq!(media_kind_of("raw.heic"), Some(LibraryMediaKind::File));
        assert_eq!(media_kind_of("notes.pdf"), None);
    }

    #[test]
    fn encodes_valid_png_and_reads_dimensions() {
        let dir = std::env::temp_dir().join(format!("zendia-media-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::create_dir_all(dir.join(".hidden")).unwrap();

        let png = encode_rgb_png(40, 24, &vec![120u8; 40 * 24 * 3]);
        let img_path = dir.join("shot.png");
        fs::write(&img_path, &png).unwrap();
        fs::write(dir.join("sub").join("clip.mp4"), b"fake-video").unwrap();
        fs::write(dir.join(".hidden").join("ignored.png"), &png).unwrap();
        fs::write(dir.join("readme.txt"), b"not media").unwrap();

        assert_eq!(read_image_dimensions(&img_path), Some((40, 24)));

        let items = scan_media_folders(
            std::slice::from_ref(&dir),
            MAX_SCAN_DEPTH,
            MAX_SCAN_FOLDERS,
            MAX_SCAN_ITEMS,
        );
        assert_eq!(items.len(), 2);
        let img = items.iter().find(|i| i.name == "shot.png").unwrap();
        assert_eq!(img.kind, LibraryMediaKind::Image);
        assert_eq!(img.dimensions, Some((40, 24)));
        let vid = items.iter().find(|i| i.name == "clip.mp4").unwrap();
        assert_eq!(vid.kind, LibraryMediaKind::Video);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn merges_downloads_and_filters_by_query_and_folder() {
        let now = 1_795_694_400;
        let (sample, _) = materialize_sample_media(now);
        assert_eq!(sample.len(), 5);

        let dl = Download {
            id: DownloadId(99),
            filename: "fresh-screenshot.png".to_string(),
            url: "https://example.org/fresh-screenshot.png".to_string(),
            bytes_received: 82_000,
            total_bytes: Some(82_000),
            status: DownloadStatus::Completed,
            target_path: Some("/tmp/fresh-screenshot.png".to_string()),
            speed_bytes_per_sec: None,
            updated_at_unix: now + 100,
            file_exists: true,
            open_when_done: false,
            simulated: false,
        };
        let from_dl = media_from_downloads(&[dl]);
        assert_eq!(from_dl.len(), 1);

        let merged = merge_media_items(sample, from_dl);
        assert_eq!(merged[0].name, "fresh-screenshot.png");

        let only_videos = filter_media_items(&merged, "", &["Videos".to_string()]);
        assert_eq!(only_videos.len(), 1);
        assert_eq!(only_videos[0].kind, LibraryMediaKind::Video);

        let by_name = filter_media_items(&merged, "aurora", &[]);
        assert_eq!(by_name.len(), 1);
        assert_eq!(by_name[0].name, "aurora-workspace.png");
    }

    #[test]
    fn steps_preview_and_formats_metadata() {
        let now = 1_795_694_400;
        let (items, _) = materialize_sample_media(now);
        assert_eq!(step_media_id(&items, items[0].id, -1), Some(items[0].id));
        assert_eq!(step_media_id(&items, items[0].id, 1), Some(items[1].id));
        assert_eq!(step_media_id(&items, items[4].id, 1), Some(items[4].id));

        assert_eq!(format_relative_date(now - 600, now), "Today");
        assert_eq!(format_relative_date(now - SECS_PER_DAY - 10, now), "Yesterday");
        assert_eq!(format_relative_date(now - 3 * SECS_PER_DAY, now), "3 days ago");
        assert_eq!(format_relative_date(now - 14 * SECS_PER_DAY, now), "2 wk. ago");
        assert_eq!(format_dimensions(Some((1920, 1080))), Some("1920 × 1080".to_string()));
        assert_eq!(format_duration(42), "0:42");
        assert_eq!(format_duration(3_665), "1:01:05");
    }
}
