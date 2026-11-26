/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The library's Media section (Zen's `ZenLibraryMediaSection`): opt-in card over a placeholder grid when disabled,
//! search and folder filter bar, 3-column thumbnail grid loaded from local image files via Freya's `ImageViewer`,
//! video tiles with play badge and duration pill, hover states, and row context menus.

use crate::ui::chrome::menus::{OpenContextMenu, try_use_menu_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::icons::prepared_icon_bytes;
use crate::ui::kit::list_nav::scroll_item_into_view;
use crate::ui::kit::{Icon, KitChip};
use crate::ui::library::nav::{LibraryCursorRing, LibraryNavLayout, LibraryNavRow, use_library_nav};
use crate::ui::library::search::{LibraryFilterChip, LibraryFilterGroup, LibrarySearchBar};
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, use_theme};
use freya::prelude::*;
use std::path::PathBuf;
use zendia_browser_core::commands::Command;
use zendia_browser_core::library::LibrarySection;
use zendia_browser_core::library_media::{
    LibraryMediaItem, LibraryMediaKind, build_media_context_menu, filter_media_items, format_duration,
};

/// Width of this section's body next to the rail (Zen's `418px - 110px`).
pub const WIDTH: f32 = 308.0;

/// Number of thumbnail columns rendered in the Media grid (`308px` fits 2 × 134px square tiles with 12px gap + 14px padding).
pub const MEDIA_GRID_COLUMNS: usize = 2;

const TILE_SIZE: f32 = 134.0;
const GRID_GAP: f32 = 12.0;

#[derive(Clone, PartialEq)]
pub struct MediaSection;

impl Component for MediaSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut query = use_state(String::new);
        let filters_open = use_state(|| false);
        let mut active_folders = use_state(Vec::<String>::new);
        let mut scroll_controller = use_scroll_controller(ScrollConfig::default);
        let mut viewport_height = use_state(|| 480.0f32);
        let mut last_scrolled_cursor = use_state(|| None::<String>);

        let (enabled, items, folders) = {
            let model = core.model().read();
            (
                model.library_media_enabled,
                model.library_media.clone(),
                model.library_media_folders.clone(),
            )
        };

        let query_text = query.read().clone();
        let folder_filter = active_folders.read().clone();
        let visible_items: Vec<LibraryMediaItem> = if enabled {
            filter_media_items(&items, &query_text, &folder_filter)
                .into_iter()
                .cloned()
                .collect()
        } else {
            Vec::new()
        };

        let nav_rows: Vec<LibraryNavRow> = if !enabled {
            vec![LibraryNavRow::new(
                "media:opt-in",
                Some(Command::LibraryMediaSetEnabled { enabled: true }),
            )]
        } else {
            visible_items
                .iter()
                .map(|item| {
                    LibraryNavRow::new(
                        format!("media:{}", item.id.0),
                        Some(Command::LibraryMediaPreviewOpen { media_id: item.id }),
                    )
                })
                .collect()
        };

        let nav_layout = if enabled {
            LibraryNavLayout::Grid {
                columns: MEDIA_GRID_COLUMNS,
            }
        } else {
            LibraryNavLayout::List
        };
        let focused_key = use_library_nav(LibrarySection::Media, nav_layout, nav_rows);

        if enabled && *last_scrolled_cursor.peek() != focused_key {
            last_scrolled_cursor.set(focused_key.clone());
            if let Some(target_key) = focused_key.as_deref()
                && let Some(idx) = visible_items
                    .iter()
                    .position(|item| format!("media:{}", item.id.0) == target_key)
            {
                let row_idx = idx / MEDIA_GRID_COLUMNS;
                let total_rows = visible_items.len().div_ceil(MEDIA_GRID_COLUMNS);
                let item_top = 6.0 + row_idx as f32 * (TILE_SIZE + GRID_GAP);
                let content_height = 6.0 + total_rows as f32 * (TILE_SIZE + GRID_GAP) + 16.0;
                let vh = *viewport_height.peek();
                scroll_item_into_view(&mut scroll_controller, item_top, TILE_SIZE, vh, content_height);
            }
        }

        if !enabled {
            let opt_in_focused = focused_key.as_deref() == Some("media:opt-in");
            return render_opt_in_state(&core, &theme, opt_in_focused);
        }

        let show_filter_button = folders.len() >= 2;
        let active_folder_count = folder_filter.len();
        let filter_groups = if show_filter_button {
            vec![LibraryFilterGroup::new(
                "folder",
                "Where to look",
                folders
                    .iter()
                    .map(|folder| {
                        let selected = folder_filter.iter().any(|f| f == folder);
                        LibraryFilterChip::new(folder.clone(), folder.clone(), selected)
                    })
                    .collect(),
            )]
        } else {
            Vec::new()
        };

        let top_bar = LibrarySearchBar::new("Search Media…", query)
            .filter_title("Filter Media…")
            .filter_groups(filter_groups)
            .filters_open(filters_open)
            .active_filter_count(active_folder_count)
            .on_toggle_filter(move |(_, folder_name): (String, String)| {
                let mut list = active_folders.write();
                if let Some(pos) = list.iter().position(|f| f == &folder_name) {
                    list.remove(pos);
                } else {
                    list.push(folder_name);
                }
            });

        let content_body: Element = if visible_items.is_empty() {
            let has_filter = !query_text.trim().is_empty() || !folder_filter.is_empty();
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .spacing(10.0)
                .padding(Gaps::new_all(24.0))
                .child(Icon::new("camera").size(28.0).role(ThemeRole::TextMuted))
                .child(
                    label()
                        .text("No media found")
                        .font_size(13.5)
                        .color(theme.color(ThemeRole::TextMuted)),
                )
                .maybe_child(has_filter.then(|| {
                    KitChip::new("Reset filters").on_press(move |_| {
                        query.set(String::new());
                        active_folders.set(Vec::new());
                    })
                }))
                .into_element()
        } else {
            let rows: Vec<Element> = visible_items
                .chunks(MEDIA_GRID_COLUMNS)
                .enumerate()
                .map(|(row_idx, chunk)| {
                    let tiles: Vec<Element> = chunk
                        .iter()
                        .map(|item| {
                            let key = format!("media:{}", item.id.0);
                            let focused = focused_key.as_deref() == Some(key.as_str());
                            MediaTile {
                                item: item.clone(),
                                focused,
                                key: DiffKey::from(&item.id.0),
                            }
                            .into_element()
                        })
                        .collect();

                    rect()
                        .key(row_idx)
                        .width(Size::fill())
                        .direction(Direction::Horizontal)
                        .spacing(GRID_GAP)
                        .children(tiles)
                        .into_element()
                })
                .collect();

            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .on_sized(move |e: Event<SizedEventData>| {
                    let h = e.area.height();
                    if (*viewport_height.peek() - h).abs() > 1.0 {
                        viewport_height.set(h);
                    }
                })
                .child(
                    ScrollView::new_controlled(scroll_controller)
                        .width(Size::fill())
                        .height(Size::fill())
                        .child(
                            rect()
                                .width(Size::fill())
                                .padding(Gaps::new(6.0, 14.0, 16.0, 14.0))
                                .spacing(GRID_GAP)
                                .children(rows),
                        ),
                )
                .into_element()
        };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .content(Content::Flex)
            .child(top_bar)
            .child(content_body)
            .into_element()
    }
}

/// Renders Zen's Media opt-in card over a tiled placeholder skeleton grid (matching `library-media-*.png`).
fn render_opt_in_state(
    core: &crate::ui::core_view::CoreView,
    theme: &crate::ui::theme::ResolvedTheme,
    button_focused: bool,
) -> Element {
    let skeleton_tile_bg = match theme.scheme {
        ResolvedScheme::Dark => Color::from_argb(20, 255, 255, 255),
        ResolvedScheme::Light => Color::from_argb(14, 0, 0, 0),
    };
    let card_bg = match theme.scheme {
        ResolvedScheme::Dark => Color::from_rgb(28, 29, 33),
        ResolvedScheme::Light => Color::from_rgb(250, 250, 252),
    };
    let card_border = match theme.scheme {
        ResolvedScheme::Dark => Color::from_argb(18, 255, 255, 255),
        ResolvedScheme::Light => Color::from_argb(18, 0, 0, 0),
    };
    let card_shadow = Shadow::new().x(0.0).y(16.0).blur(42.0).color(Color::from_argb(
        if theme.scheme.is_dark() { 130 } else { 52 },
        0,
        0,
        0,
    ));

    let skeleton_rows: Vec<Element> = (0..6)
        .map(|row_idx| {
            rect()
                .key(row_idx)
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .spacing(12.0)
                .children((0..MEDIA_GRID_COLUMNS).map(|col_idx| {
                    rect()
                        .key(col_idx)
                        .width(Size::flex(1.0))
                        .height(Size::px(TILE_SIZE))
                        .corner_radius(CornerRadius::new_all(16.0))
                        .background(skeleton_tile_bg)
                }))
                .into_element()
        })
        .collect();

    let enable_core = core.clone();
    let button_bg = match theme.scheme {
        ResolvedScheme::Dark => Rgba::rgb(90, 92, 118).to_color(),
        ResolvedScheme::Light => Rgba::rgb(122, 118, 178).to_color(),
    };
    let button_fg = Color::WHITE;
    let show_button = LibraryCursorRing::new(
        button_focused,
        rect()
            .width(Size::fill())
            .height(Size::px(40.0))
            .corner_radius(CornerRadius::new_all(10.0))
            .background(button_bg)
            .center()
            .cursor(CursorIcon::Pointer)
            .on_press(move |_| {
                enable_core.invoke_or_log(Command::LibraryMediaSetEnabled { enabled: true });
            })
            .child(
                label()
                    .text("Show Media")
                    .font_size(14.0)
                    .font_weight(FontWeight::BOLD)
                    .color(button_fg),
            ),
    )
    .corner_radius(10.0)
    .fill(false);

    let card = rect()
        .width(Size::fill())
        .max_width(Size::px(276.0))
        .padding(Gaps::new(22.0, 20.0, 18.0, 20.0))
        .corner_radius(CornerRadius::new_all(22.0))
        .background(card_bg)
        .border(
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(card_border),
        )
        .shadow(card_shadow)
        .cross_align(Alignment::center())
        .spacing(12.0)
        .child(Icon::new("library-media").size(40.0).role(ThemeRole::Text))
        .child(
            label()
                .width(Size::fill())
                .text("Your Media in the Library")
                .font_size(15.0)
                .font_weight(FontWeight::BOLD)
                .text_align(TextAlign::Center)
                .color(theme.color(ThemeRole::Text)),
        )
        .child(
            label()
                .width(Size::fill())
                .text("Quickly grab screenshots, images and videos from your computer by showing them in the Zendia Library.")
                .font_size(13.0)
                .text_align(TextAlign::Center)
                .color(theme.color(ThemeRole::TextMuted)),
        )
        .child(rect().width(Size::fill()).padding(Gaps::new(4.0, 0.0, 2.0, 0.0)).child(show_button))
        .child(
            rect()
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .cross_align(Alignment::center())
                .spacing(6.0)
                .child(Icon::new("lock").size(13.0).role(ThemeRole::TextMuted))
                .child(
                    label()
                        .width(Size::flex(1.0))
                        .text("Media never leaves your computer and is not viewable by anyone but you.")
                        .font_size(11.0)
                        .color(theme.color(ThemeRole::TextMuted)),
                ),
        );

    rect()
        .width(Size::fill())
        .height(Size::fill())
        .overflow(Overflow::Clip)
        .child(
            rect()
                .position(Position::new_absolute().top(0.0).left(0.0))
                .width(Size::percent(100.0))
                .height(Size::percent(100.0))
                .padding(Gaps::new(12.0, 14.0, 12.0, 14.0))
                .spacing(12.0)
                .opacity(0.75)
                .children(skeleton_rows),
        )
        .child(
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .padding(Gaps::new_all(18.0))
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .child(card),
        )
        .into_element()
}

#[derive(Clone, PartialEq)]
struct MediaTile {
    item: LibraryMediaItem,
    focused: bool,
    key: DiffKey,
}

impl KeyExt for MediaTile {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for MediaTile {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let menu_state = try_use_menu_state();
        let mut hovered = use_state(|| false);

        let is_hovered = *hovered.read();
        let item = &self.item;
        let media_id = item.id;

        let tile_bg = match (theme.scheme, is_hovered) {
            (ResolvedScheme::Dark, false) => Color::from_argb(20, 255, 255, 255),
            (ResolvedScheme::Dark, true) => Color::from_argb(38, 255, 255, 255),
            (ResolvedScheme::Light, false) => Color::from_argb(15, 0, 0, 0),
            (ResolvedScheme::Light, true) => Color::from_argb(30, 0, 0, 0),
        };

        let open_core = core.clone();
        let on_press = move |_| {
            open_core.invoke_or_log(Command::LibraryMediaPreviewOpen { media_id });
        };

        let menu_item = item.clone();
        let on_mouse_down = move |e: Event<MouseEventData>| {
            if e.button == Some(MouseButton::Right) {
                e.stop_propagation();
                if let Some(ms) = menu_state {
                    ms.open_context_menu(OpenContextMenu {
                        position: (e.global_location.x as f32, e.global_location.y as f32),
                        items: build_media_context_menu(&menu_item),
                        tab_id: None,
                        open_submenu: None,
                    });
                }
            }
        };

        let inner_media = render_tile_media(item, &theme);

        let tile = rect()
            .width(Size::px(TILE_SIZE))
            .height(Size::px(TILE_SIZE))
            .corner_radius(CornerRadius::new_all(16.0))
            .background(tile_bg)
            .padding(Gaps::new_all(12.0))
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set(true))
            .on_pointer_leave(move |_| hovered.set(false))
            .on_mouse_down(on_mouse_down)
            .on_press(on_press)
            .child(inner_media)
            .maybe_child(is_hovered.then(|| {
                rect()
                    .position(Position::new_absolute().bottom(6.0).left(6.0).right(6.0))
                    .padding(Gaps::new(2.0, 6.0, 2.0, 6.0))
                    .corner_radius(CornerRadius::new_all(6.0))
                    .background(Color::from_argb(185, 16, 18, 26))
                    .child(
                        label()
                            .text(item.name.clone())
                            .font_size(10.5)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis)
                            .color(Color::WHITE),
                    )
            }));

        rect().width(Size::px(TILE_SIZE)).height(Size::px(TILE_SIZE)).child(
            LibraryCursorRing::new(self.focused, tile)
                .corner_radius(16.0)
                .fill(false),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

fn render_tile_media(item: &LibraryMediaItem, theme: &crate::ui::theme::ResolvedTheme) -> Element {
    let thumb_size = TILE_SIZE - 24.0;
    let thumb_shadow = Shadow::new()
        .x(0.0)
        .y(2.0)
        .blur(10.0)
        .color(Color::from_argb(64, 0, 0, 0));

    match item.kind {
        LibraryMediaKind::Image => {
            let path = PathBuf::from(&item.path);
            if path.is_file() {
                rect()
                    .width(Size::px(thumb_size))
                    .height(Size::px(thumb_size))
                    .corner_radius(CornerRadius::new_all(8.0))
                    .overflow(Overflow::Clip)
                    .shadow(thumb_shadow)
                    .child(
                        ImageViewer::new(path)
                            .width(Size::px(thumb_size))
                            .height(Size::px(thumb_size))
                            .aspect_ratio(AspectRatio::Min)
                            .image_cover(ImageCover::Center),
                    )
                    .into_element()
            } else {
                rect()
                    .width(Size::px(thumb_size))
                    .height(Size::px(thumb_size))
                    .corner_radius(CornerRadius::new_all(8.0))
                    .background(theme.color(ThemeRole::SurfaceRaised))
                    .center()
                    .child(Icon::new("camera").size(26.0).role(ThemeRole::TextMuted))
                    .into_element()
            }
        }
        LibraryMediaKind::Video => {
            let poster = item
                .thumbnail_path
                .as_deref()
                .map(PathBuf::from)
                .filter(|p| p.is_file());

            let backdrop: Element = if let Some(poster_path) = poster {
                ImageViewer::new(poster_path)
                    .width(Size::px(thumb_size))
                    .height(Size::px(thumb_size))
                    .aspect_ratio(AspectRatio::Max)
                    .image_cover(ImageCover::Center)
                    .into_element()
            } else {
                rect()
                    .width(Size::px(thumb_size))
                    .height(Size::px(thumb_size))
                    .background(Color::from_rgb(28, 32, 46))
                    .into_element()
            };

            let play_icon: Element = if let Some((canonical, bytes)) = prepared_icon_bytes("media-play") {
                SvgViewer::new((("zendia-icon-white", canonical), bytes))
                    .width(Size::px(15.0))
                    .height(Size::px(15.0))
                    .color(Color::WHITE)
                    .show_loader(false)
                    .into_element()
            } else {
                Icon::new("media-play").size(15.0).role(ThemeRole::Text).into_element()
            };

            let play_badge = rect()
                .position(
                    Position::new_absolute()
                        .top((thumb_size - 34.0) * 0.5)
                        .left((thumb_size - 34.0) * 0.5),
                )
                .width(Size::px(34.0))
                .height(Size::px(34.0))
                .corner_radius(CornerRadius::new_all(17.0))
                .background(Color::from_argb(145, 0, 0, 0))
                .center()
                .child(play_icon);

            let duration_pill = item.duration_secs.map(|secs| {
                rect()
                    .position(Position::new_absolute().bottom(4.0).right(4.0))
                    .padding(Gaps::new(1.5, 5.0, 1.5, 5.0))
                    .corner_radius(CornerRadius::new_all(5.0))
                    .background(Color::from_argb(170, 0, 0, 0))
                    .child(
                        label()
                            .text(format_duration(secs))
                            .font_size(10.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(Color::WHITE),
                    )
            });

            rect()
                .width(Size::px(thumb_size))
                .height(Size::px(thumb_size))
                .corner_radius(CornerRadius::new_all(8.0))
                .overflow(Overflow::Clip)
                .shadow(thumb_shadow)
                .child(backdrop)
                .child(play_badge)
                .maybe_child(duration_pill)
                .into_element()
        }
        LibraryMediaKind::File => rect()
            .width(Size::px(thumb_size))
            .height(Size::px(thumb_size))
            .center()
            .spacing(6.0)
            .child(Icon::new("page-portrait").size(30.0).role(ThemeRole::TextMuted))
            .child(
                label()
                    .text(item.extension.to_ascii_uppercase())
                    .font_size(11.0)
                    .font_weight(FontWeight::BOLD)
                    .color(theme.color(ThemeRole::TextMuted)),
            )
            .into_element(),
    }
}
