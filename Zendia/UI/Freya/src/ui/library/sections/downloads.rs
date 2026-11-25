/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The library's Downloads section (Zen's `ZenLibraryDownloadsSection`): search bar, file-type and recency filters,
//! day-grouped rows (newest first) with circular progress ring, inline progress bar, image thumbnails, hover URL
//! reveal, action buttons, and row context menu.

use crate::ui::chrome::menus::{OpenContextMenu, try_use_menu_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, IconButton, KitChip};
use crate::ui::library::nav::{LibraryCursorRing, LibraryNavLayout, LibraryNavRow, use_library_nav};
use crate::ui::library::search::LibrarySearchBar;
use crate::ui::theme::{ResolvedScheme, ThemeRole, use_theme};
use freya::engine::prelude::{Canvas, Color as SkColor, Paint, PaintCap, PaintStyle, PathBuilder, Point as SkPoint};
use freya::prelude::*;
use std::f32::consts::PI;
use std::path::PathBuf;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::downloads::{
    self, DownloadFileGroup, DownloadFilter, DownloadWhenFilter, build_download_context_menu, can_cancel, can_pause,
    can_resume, can_retry, current_unix_seconds, file_group_of, format_display_url, format_status_line,
    group_downloads_by_day, has_file, is_finished, is_pending, progress_fraction,
};
use zendia_browser_core::library::LibrarySection;
use zendia_browser_core::model::{Download, DownloadId, DownloadStatus};

/// Width of this section's body next to the rail.
pub const WIDTH: f32 = 320.0;

const DOWNLOAD_TICK_MS: u64 = 250;

/// Optional view-local filter/search state for the Downloads section so gallery fixtures can showcase filtered views.
#[derive(Clone, Copy, PartialEq)]
pub struct DownloadsViewState {
    pub query: State<String>,
    pub filters_open: State<bool>,
    pub file_groups: State<Vec<DownloadFileGroup>>,
    pub when: State<Option<DownloadWhenFilter>>,
}

/// Provides optional external [`DownloadsViewState`] (used by the gallery to pin search/filter states).
pub fn use_provide_downloads_view_state(
    initial_query: impl Into<String>,
    initial_filters_open: bool,
    initial_groups: Vec<DownloadFileGroup>,
    initial_when: Option<DownloadWhenFilter>,
) -> DownloadsViewState {
    let initial_query = initial_query.into();
    let query = use_state(move || initial_query);
    let filters_open = use_state(move || initial_filters_open);
    let file_groups = use_state(move || initial_groups);
    let when = use_state(move || initial_when);
    use_provide_context(|| DownloadsViewState {
        query,
        filters_open,
        file_groups,
        when,
    })
}

#[derive(Clone, PartialEq)]
pub struct DownloadsSection;

impl Component for DownloadsSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let provided = try_consume_context::<DownloadsViewState>();
        let local_query = use_state(String::new);
        let local_filters_open = use_state(|| false);
        let local_groups = use_state(Vec::<DownloadFileGroup>::new);
        let local_when = use_state(|| None::<DownloadWhenFilter>);

        let mut query = provided.map(|s| s.query).unwrap_or(local_query);
        let mut filters_open = provided.map(|s| s.filters_open).unwrap_or(local_filters_open);
        let mut selected_groups = provided.map(|s| s.file_groups).unwrap_or(local_groups);
        let mut selected_when = provided.map(|s| s.when).unwrap_or(local_when);

        // Advance any simulated sample downloads while this section is mounted.
        use_hook({
            let core = core.clone();
            move || {
                spawn_in_window(async move {
                    loop {
                        timer(Duration::from_millis(DOWNLOAD_TICK_MS)).await;
                        let has_simulated_active = core
                            .model()
                            .peek()
                            .downloads
                            .iter()
                            .any(|d| d.simulated && d.status == DownloadStatus::InProgress);
                        if has_simulated_active {
                            core.invoke_or_log(Command::DownloadTick);
                        }
                    }
                });
            }
        });

        let downloads = core.model().read().downloads.clone();
        let now_unix = current_unix_seconds();
        let filter = DownloadFilter {
            query: query.read().clone(),
            file_groups: selected_groups.read().clone(),
            when: *selected_when.read(),
        };
        let day_groups = group_downloads_by_day(&downloads, &filter, now_unix);

        let nav_rows: Vec<LibraryNavRow> = day_groups
            .iter()
            .flat_map(|group| group.downloads.iter())
            .map(|download| {
                let activate = if download.status == DownloadStatus::InProgress {
                    Some(Command::DownloadToggleOpenWhenDone {
                        download_id: download.id,
                    })
                } else if download.status == DownloadStatus::Paused {
                    Some(Command::DownloadResume {
                        download_id: download.id,
                    })
                } else if can_retry(download) {
                    Some(Command::DownloadRetry {
                        download_id: download.id,
                    })
                } else {
                    Some(Command::DownloadOpen {
                        download_id: download.id,
                    })
                };
                LibraryNavRow::new(format!("download:{}", download.id.0), activate)
            })
            .collect();

        let focused_key = use_library_nav(LibrarySection::Downloads, LibraryNavLayout::List, nav_rows);

        let is_filters_open = *filters_open.read();
        let active_filter_count = filter.active_filter_count();
        let has_finished = downloads.iter().any(is_finished);

        let pill_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(22, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(18, 0, 0, 0),
        };
        let pill_active_bg = theme.rgba(ThemeRole::Accent).with_alpha(0.24).to_color();

        let top_bar = if !is_filters_open {
            rect()
                .width(Size::fill())
                .padding(Gaps::new(10.0, 12.0, 6.0, 10.0))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .cross_align(Alignment::center())
                .spacing(6.0)
                .child(
                    rect()
                        .width(Size::flex(1.0))
                        .child(LibrarySearchBar::new("Search Downloads…", query)),
                )
                .child(
                    rect()
                        .height(Size::px(34.0))
                        .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
                        .corner_radius(CornerRadius::new_all(10.0))
                        .background(if active_filter_count > 0 {
                            pill_active_bg
                        } else {
                            pill_bg
                        })
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::center())
                        .spacing(6.0)
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |_| {
                            filters_open.set(true);
                        })
                        .child(
                            Icon::new("circle-bars-filter")
                                .size(15.0)
                                .role(if active_filter_count > 0 {
                                    ThemeRole::Accent
                                } else {
                                    ThemeRole::TextMuted
                                }),
                        )
                        .child(
                            label()
                                .text(if active_filter_count > 0 {
                                    format!("Filter ({active_filter_count})")
                                } else {
                                    "Filter".to_string()
                                })
                                .font_size(13.0)
                                .font_weight(FontWeight::MEDIUM)
                                .color(if active_filter_count > 0 {
                                    theme.color(ThemeRole::Accent)
                                } else {
                                    theme.color(ThemeRole::Text)
                                }),
                        ),
                )
                .into_element()
        } else {
            let current_groups = selected_groups.read().clone();
            let current_when = *selected_when.read();

            rect()
                .width(Size::fill())
                .padding(Gaps::new(12.0, 14.0, 10.0, 14.0))
                .spacing(12.0)
                .child(
                    rect()
                        .width(Size::fill())
                        .direction(Direction::Horizontal)
                        .main_align(Alignment::SpaceBetween)
                        .cross_align(Alignment::center())
                        .child(
                            label()
                                .text("Filter Downloads…")
                                .font_size(14.0)
                                .font_weight(FontWeight::BOLD)
                                .color(theme.color(ThemeRole::Text)),
                        )
                        .child(
                            rect()
                                .padding(Gaps::new(6.0, 14.0, 6.0, 14.0))
                                .corner_radius(CornerRadius::new_all(10.0))
                                .background(pill_bg)
                                .cursor(CursorIcon::Pointer)
                                .on_press(move |_| {
                                    filters_open.set(false);
                                })
                                .child(
                                    label()
                                        .text("Done")
                                        .font_size(13.0)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .color(theme.color(ThemeRole::Text)),
                                ),
                        ),
                )
                .child(
                    rect()
                        .width(Size::fill())
                        .spacing(8.0)
                        .child(
                            label()
                                .text("File type")
                                .font_size(12.5)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .color(theme.color(ThemeRole::TextMuted)),
                        )
                        .child(
                            rect()
                                .width(Size::fill())
                                .direction(Direction::Horizontal)
                                .content(Content::wrap())
                                .spacing(6.0)
                                .children(DownloadFileGroup::ALL.into_iter().map(|group| {
                                    let active = current_groups.contains(&group);
                                    KitChip::new(group.label())
                                        .key(group.id())
                                        .selected(active)
                                        .on_press(move |_| {
                                            let mut list = selected_groups.write();
                                            if let Some(pos) = list.iter().position(|g| *g == group) {
                                                list.remove(pos);
                                            } else {
                                                list.push(group);
                                            }
                                        })
                                })),
                        ),
                )
                .child(
                    rect()
                        .width(Size::fill())
                        .spacing(8.0)
                        .child(
                            label()
                                .text("When was it downloaded?")
                                .font_size(12.5)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .color(theme.color(ThemeRole::TextMuted)),
                        )
                        .child(
                            rect()
                                .width(Size::fill())
                                .direction(Direction::Horizontal)
                                .content(Content::wrap())
                                .spacing(6.0)
                                .children(DownloadWhenFilter::ALL.into_iter().map(|when_opt| {
                                    let active = current_when == Some(when_opt);
                                    KitChip::new(when_opt.label())
                                        .key(when_opt.id())
                                        .selected(active)
                                        .on_press(move |_| {
                                            let next = if *selected_when.peek() == Some(when_opt) {
                                                None
                                            } else {
                                                Some(when_opt)
                                            };
                                            selected_when.set(next);
                                        })
                                })),
                        ),
                )
                .maybe_child((active_filter_count > 0).then(|| {
                    rect()
                        .width(Size::fill())
                        .child(KitChip::new("Reset filters").on_press(move |_| {
                            selected_groups.set(Vec::new());
                            selected_when.set(None);
                        }))
                }))
                .into_element()
        };

        let clear_core = core.clone();
        let sub_bar = (has_finished || active_filter_count > 0).then(|| {
            rect()
                .width(Size::fill())
                .padding(Gaps::new(0.0, 14.0, 4.0, 14.0))
                .direction(Direction::Horizontal)
                .main_align(Alignment::SpaceBetween)
                .cross_align(Alignment::center())
                .child(if active_filter_count > 0 {
                    label()
                        .text(format!(
                            "{} matching",
                            day_groups.iter().map(|g| g.downloads.len()).sum::<usize>()
                        ))
                        .font_size(11.5)
                        .color(theme.color(ThemeRole::TextMuted))
                        .into_element()
                } else {
                    rect().into_element()
                })
                .maybe_child(has_finished.then(|| {
                    rect()
                        .padding(Gaps::new(3.0, 8.0, 3.0, 8.0))
                        .corner_radius(CornerRadius::new_all(6.0))
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |_| {
                            clear_core.invoke_or_log(Command::DownloadClearFinished);
                        })
                        .child(
                            label()
                                .text("Clear finished")
                                .font_size(11.5)
                                .font_weight(FontWeight::MEDIUM)
                                .color(theme.color(ThemeRole::TextMuted)),
                        )
                }))
        });

        let list_body: Element = if downloads.is_empty() {
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .spacing(10.0)
                .padding(Gaps::new_all(24.0))
                .child(Icon::new("downloads").size(28.0).role(ThemeRole::TextMuted))
                .child(
                    label()
                        .text("No downloads found")
                        .font_size(13.5)
                        .color(theme.color(ThemeRole::TextMuted)),
                )
                .into_element()
        } else if day_groups.is_empty() {
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .spacing(10.0)
                .padding(Gaps::new_all(24.0))
                .child(Icon::new("search-glass").size(24.0).role(ThemeRole::TextMuted))
                .child(
                    label()
                        .text("No matching downloads")
                        .font_size(13.5)
                        .color(theme.color(ThemeRole::TextMuted)),
                )
                .child(KitChip::new("Reset search & filters").on_press(move |_| {
                    query.set(String::new());
                    selected_groups.set(Vec::new());
                    selected_when.set(None);
                }))
                .into_element()
        } else {
            let group_nodes: Vec<Element> = day_groups
                .into_iter()
                .map(|group| {
                    let row_nodes: Vec<Element> = group
                        .downloads
                        .into_iter()
                        .map(|download| {
                            let row_key = format!("download:{}", download.id.0);
                            let focused = focused_key.as_deref() == Some(row_key.as_str());
                            DownloadRow {
                                download: download.clone(),
                                now_unix,
                                focused,
                                key: DiffKey::from(&download.id.0),
                            }
                            .into_element()
                        })
                        .collect();

                    rect()
                        .key(group.day_index)
                        .width(Size::fill())
                        .spacing(2.0)
                        .padding(Gaps::new(4.0, 0.0, 6.0, 0.0))
                        .child(
                            rect().padding(Gaps::new(6.0, 10.0, 4.0, 10.0)).child(
                                label()
                                    .text(group.label)
                                    .font_size(11.5)
                                    .font_weight(FontWeight::BOLD)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            ),
                        )
                        .children(row_nodes)
                        .into_element()
                })
                .collect();

            ScrollView::new()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(0.0, 8.0, 12.0, 8.0))
                        .spacing(2.0)
                        .children(group_nodes),
                )
                .into_element()
        };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .content(Content::Flex)
            .child(top_bar)
            .maybe_child(sub_bar)
            .child(list_body)
    }
}

#[derive(Clone, PartialEq)]
struct DownloadRow {
    download: Download,
    now_unix: u64,
    focused: bool,
    key: DiffKey,
}

impl KeyExt for DownloadRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for DownloadRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let menu_state = try_use_menu_state();
        let mut hovered = use_state(|| false);
        let mut row_area = use_state(|| (0.0f32, 0.0f32, 0.0f32, 0.0f32));

        let is_hovered = *hovered.read();
        let download = &self.download;
        let download_id = download.id;
        let pending = is_pending(download);
        let missing = download.status == DownloadStatus::Completed && !download.file_exists;
        let paused = download.status == DownloadStatus::Paused;
        let progress = progress_fraction(download);

        let status_line = if is_hovered && !pending {
            format_display_url(&download.url)
        } else {
            format_status_line(download, self.now_unix)
        };

        let status_color = match &download.status {
            DownloadStatus::Failed { .. } => theme.color(ThemeRole::Danger),
            _ if download.open_when_done && download.status == DownloadStatus::InProgress => {
                theme.color(ThemeRole::Accent)
            }
            _ => theme.color(ThemeRole::TextMuted),
        };

        let icon_box = build_download_icon_box(download, &theme);

        let press_core = core.clone();
        let press_status = download.status.clone();
        let on_row_press = move |_| {
            let cmd = match press_status {
                DownloadStatus::InProgress => Command::DownloadToggleOpenWhenDone { download_id },
                DownloadStatus::Paused => Command::DownloadResume { download_id },
                DownloadStatus::Failed { .. } | DownloadStatus::Cancelled => Command::DownloadRetry { download_id },
                DownloadStatus::Completed => Command::DownloadOpen { download_id },
            };
            press_core.invoke_or_log(cmd);
        };

        let ctx_download = download.clone();
        let on_mouse_down = move |e: Event<MouseEventData>| {
            if e.button == Some(MouseButton::Right) {
                e.stop_propagation();
                if let Some(ms) = menu_state {
                    ms.open_context_menu(OpenContextMenu {
                        position: (e.global_location.x as f32, e.global_location.y as f32),
                        items: build_download_context_menu(&ctx_download),
                        tab_id: None,
                        open_submenu: None,
                    });
                }
            }
        };

        // Action buttons: active/paused/failed controls stay visible; completed show-in-folder & more appear on hover/focus.
        let show_controls = is_hovered || self.focused;
        let actions = build_row_actions(download, show_controls, &core, menu_state, row_area);

        let row_bg = if is_hovered {
            theme.color(ThemeRole::Hover)
        } else {
            Color::TRANSPARENT
        };

        let content = rect()
            .width(Size::fill())
            .padding(Gaps::new(9.0, 10.0, 9.0, 10.0))
            .corner_radius(CornerRadius::new_all(14.0))
            .background(row_bg)
            .maybe(missing, |r| r.opacity(0.6))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .spacing(10.0)
            .cursor(CursorIcon::Pointer)
            .on_sized(move |e: Event<SizedEventData>| {
                let next = (e.area.min_x(), e.area.min_y(), e.area.width(), e.area.height());
                if *row_area.peek() != next {
                    row_area.set(next);
                }
            })
            .on_pointer_enter(move |_| hovered.set(true))
            .on_pointer_leave(move |_| hovered.set(false))
            .on_mouse_down(on_mouse_down)
            .on_press(on_row_press)
            .child(icon_box)
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .spacing(3.0)
                    .overflow(Overflow::Clip)
                    .child(
                        label()
                            .text(download.filename.clone())
                            .font_size(13.5)
                            .font_weight(FontWeight::MEDIUM)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis)
                            .color(theme.color(ThemeRole::Text)),
                    )
                    .child(
                        label()
                            .text(status_line)
                            .font_size(11.5)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis)
                            .color(status_color),
                    )
                    .maybe_child(pending.then(|| build_progress_bar(progress, paused, &theme))),
            )
            .child(actions);

        LibraryCursorRing::new(self.focused, content).corner_radius(14.0)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

fn build_row_actions(
    download: &Download,
    show_secondary: bool,
    core: &crate::ui::core_view::CoreView,
    menu_state: Option<crate::ui::chrome::menus::MenuViewState>,
    row_area: State<(f32, f32, f32, f32)>,
) -> Element {
    let download_id: DownloadId = download.id;
    let mut actions = rect()
        .direction(Direction::Horizontal)
        .cross_align(Alignment::center())
        .spacing(2.0);

    if can_pause(download) {
        let pause_core = core.clone();
        actions = actions.child(
            IconButton::new("media-pause", "Pause download")
                .size(26.0)
                .icon_size(13.0)
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    pause_core.invoke_or_log(Command::DownloadPause { download_id });
                }),
        );
    }

    if can_resume(download) {
        let resume_core = core.clone();
        actions = actions.child(
            IconButton::new("media-play", "Resume download")
                .size(26.0)
                .icon_size(13.0)
                .role(ThemeRole::Accent)
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    resume_core.invoke_or_log(Command::DownloadResume { download_id });
                }),
        );
    }

    if can_cancel(download) {
        let cancel_core = core.clone();
        actions = actions.child(
            IconButton::new("close", "Cancel download")
                .size(26.0)
                .icon_size(13.0)
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    cancel_core.invoke_or_log(Command::DownloadCancel { download_id });
                }),
        );
    }

    if can_retry(download) {
        let retry_core = core.clone();
        actions = actions.child(
            IconButton::new("reload", "Retry download")
                .size(26.0)
                .icon_size(13.0)
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    retry_core.invoke_or_log(Command::DownloadRetry { download_id });
                }),
        );
    }

    if show_secondary && has_file(download) {
        let show_core = core.clone();
        actions = actions.child(
            IconButton::new("folder", "Show in folder")
                .size(26.0)
                .icon_size(14.0)
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    show_core.invoke_or_log(Command::DownloadShowInFolder { download_id });
                }),
        );
    }

    let menu_download = download.clone();
    actions = actions.child(
        IconButton::new("menu", "More options")
            .size(26.0)
            .icon_size(14.0)
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                if let Some(ms) = menu_state {
                    let (x, y, w, h) = *row_area.peek();
                    ms.open_context_menu(OpenContextMenu {
                        position: (x + w - 24.0, y + h),
                        items: build_download_context_menu(&menu_download),
                        tab_id: None,
                        open_submenu: None,
                    });
                }
            }),
    );

    actions.into_element()
}

fn build_progress_bar(fraction: Option<f32>, paused: bool, theme: &crate::ui::theme::ResolvedTheme) -> Element {
    let track = match theme.scheme {
        ResolvedScheme::Dark => Color::from_argb(32, 255, 255, 255),
        ResolvedScheme::Light => Color::from_argb(26, 0, 0, 0),
    };
    let accent = theme.rgba(ThemeRole::Accent);
    let fill_color = if paused {
        accent.with_alpha(0.45).to_color()
    } else {
        accent.to_color()
    };
    let pct = fraction.unwrap_or(0.32).clamp(0.04, 1.0) * 100.0;

    rect()
        .width(Size::fill())
        .height(Size::px(3.5))
        .corner_radius(CornerRadius::new_all(2.0))
        .overflow(Overflow::Clip)
        .background(track)
        .child(
            rect()
                .width(Size::percent(pct))
                .height(Size::fill())
                .corner_radius(CornerRadius::new_all(2.0))
                .background(fill_color),
        )
        .into_element()
}

fn build_download_icon_box(download: &Download, theme: &crate::ui::theme::ResolvedTheme) -> Element {
    if is_pending(download) {
        let fraction = progress_fraction(download).unwrap_or(0.25);
        let paused = download.status == DownloadStatus::Paused;
        let accent = theme.rgba(ThemeRole::Accent);
        let arc_color = if paused {
            accent.with_alpha(0.5).to_color()
        } else {
            accent.to_color()
        };
        let track_color = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(36, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(30, 0, 0, 0),
        };
        let sk_arc = SkColor::from_argb(arc_color.a(), arc_color.r(), arc_color.g(), arc_color.b());
        let sk_track = SkColor::from_argb(track_color.a(), track_color.r(), track_color.g(), track_color.b());

        let ring = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_progress_ring(canvas, ctx.size.width, ctx.size.height, fraction, sk_track, sk_arc);
            }
        });

        let inner_icon = if paused { "media-pause" } else { "arrow-down" };
        return rect()
            .width(Size::px(34.0))
            .height(Size::px(34.0))
            .center()
            .background(Fill::Callback(ring))
            .child(Icon::new(inner_icon).size(13.0).role(ThemeRole::Accent))
            .into_element();
    }

    if has_file(download)
        && downloads::can_draw_thumbnail(&download.filename)
        && let Some(path) = download.target_path.as_deref()
        && PathBuf::from(path).is_file()
    {
        return rect()
            .width(Size::px(34.0))
            .height(Size::px(34.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .overflow(Overflow::Clip)
            .child(
                ImageViewer::new(PathBuf::from(path))
                    .width(Size::px(34.0))
                    .height(Size::px(34.0))
                    .aspect_ratio(AspectRatio::Max)
                    .image_cover(ImageCover::Center),
            )
            .into_element();
    }

    let (icon_name, role) = match &download.status {
        DownloadStatus::Failed { .. } => ("warning", ThemeRole::Danger),
        DownloadStatus::Cancelled => ("close", ThemeRole::TextMuted),
        _ => match file_group_of(&download.filename) {
            Some(DownloadFileGroup::Images) => ("camera", ThemeRole::Accent),
            Some(DownloadFileGroup::Video) => ("media-play", ThemeRole::Accent),
            Some(DownloadFileGroup::Audio) => ("music", ThemeRole::Accent),
            Some(DownloadFileGroup::Documents) => ("page-portrait", ThemeRole::Text),
            Some(DownloadFileGroup::Archives) => ("folder", ThemeRole::Text),
            Some(DownloadFileGroup::Apps) => ("bolt", ThemeRole::Text),
            None => ("downloads", ThemeRole::TextMuted),
        },
    };

    let tile_bg = match theme.scheme {
        ResolvedScheme::Dark => Color::from_argb(24, 255, 255, 255),
        ResolvedScheme::Light => Color::from_argb(18, 0, 0, 0),
    };

    rect()
        .width(Size::px(34.0))
        .height(Size::px(34.0))
        .corner_radius(CornerRadius::new_all(8.0))
        .background(tile_bg)
        .center()
        .child(Icon::new(icon_name).size(17.0).role(role))
        .into_element()
}

fn ring_sin(mut x: f32) -> f32 {
    x = (x + PI).rem_euclid(2.0 * PI) - PI;
    let b = 4.0 / PI;
    let c = -4.0 / (PI * PI);
    let y = b * x + c * x * x.abs();
    0.225 * (y * y.abs() - y) + y
}

fn ring_cos(x: f32) -> f32 {
    ring_sin(x + PI * 0.5)
}

fn paint_progress_ring(canvas: &Canvas, width: f32, height: f32, fraction: f32, track: SkColor, arc: SkColor) {
    let cx = width * 0.5;
    let cy = height * 0.5;
    let radius = (width.min(height) * 0.5) - 2.5;

    let mut track_paint = Paint::default();
    track_paint.set_anti_alias(true);
    track_paint.set_style(PaintStyle::Stroke);
    track_paint.set_stroke_width(2.6);
    track_paint.set_color(track);
    canvas.draw_circle(SkPoint::new(cx, cy), radius, &track_paint);

    let sweep = (fraction.clamp(0.05, 1.0)) * 2.0 * PI;
    let start = -PI * 0.5;
    let steps = 32usize;
    let mut pb = PathBuilder::new();
    for step in 0..=steps {
        let ang = start + sweep * (step as f32 / steps as f32);
        let pt = SkPoint::new(cx + radius * ring_cos(ang), cy + radius * ring_sin(ang));
        if step == 0 {
            pb.move_to(pt);
        } else {
            pb.line_to(pt);
        }
    }

    let mut arc_paint = Paint::default();
    arc_paint.set_anti_alias(true);
    arc_paint.set_style(PaintStyle::Stroke);
    arc_paint.set_stroke_cap(PaintCap::Round);
    arc_paint.set_stroke_width(2.8);
    arc_paint.set_color(arc);
    canvas.draw_path(&pb.detach(), &arc_paint);
}
