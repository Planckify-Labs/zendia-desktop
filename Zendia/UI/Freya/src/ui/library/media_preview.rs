/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Full-size media preview overlay (Zen's `ZenLibraryMediaPreview`), floated over the window at
//! [`POPUP_OVERLAY_LEVEL`] with Zen's 250 ms ease-out open/close motion, left/right navigation (buttons and arrow
//! keys), Escape to close, file metadata strip, and actions.

use crate::ui::chrome::menus::{OpenContextMenu, try_use_menu_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::icons::prepared_icon_bytes;
use crate::ui::kit::{Icon, IconButton};
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::path::PathBuf;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::downloads::{current_unix_seconds, format_bytes};
use zendia_browser_core::events::ViewRequest;
use zendia_browser_core::library_media::{
    LibraryMediaItem, LibraryMediaKind, build_media_context_menu, default_opener_name, format_dimensions,
    format_duration, format_relative_date, step_media_id,
};
use zendia_browser_core::model::LibraryMediaId;

/// Zen's `MORPH_MS` open and close duration (`250 ms`).
const PREVIEW_MOTION: Duration = Duration::from_millis(250);

/// Per-window view state for the library's full-size media preview overlay.
#[derive(Clone, Copy, PartialEq)]
pub struct MediaPreviewState {
    pub open: State<bool>,
    pub current_id: State<Option<LibraryMediaId>>,
}

impl MediaPreviewState {
    pub fn open_item(&self, media_id: LibraryMediaId) {
        let mut current = self.current_id;
        current.set_if_modified(Some(media_id));
        let mut open = self.open;
        open.set_if_modified(true);
    }

    pub fn close(&self) {
        let mut open = self.open;
        open.set_if_modified(false);
    }

    pub fn step(&self, delta: i32, items: &[LibraryMediaItem]) {
        if !*self.open.peek() {
            return;
        }
        let Some(current) = *self.current_id.peek() else {
            return;
        };
        if let Some(next_id) = step_media_id(items, current, delta) {
            let mut id_state = self.current_id;
            id_state.set_if_modified(Some(next_id));
        }
    }

    pub fn is_open(&self) -> bool {
        *self.open.read()
    }

    pub fn handle_view_request(&self, request: &ViewRequest, items: &[LibraryMediaItem]) -> bool {
        match request {
            ViewRequest::LibraryMediaPreviewOpen { media_id } => self.open_item(*media_id),
            ViewRequest::LibraryMediaPreviewClose => self.close(),
            ViewRequest::LibraryMediaPreviewStep { delta } => self.step(*delta, items),
            _ => return false,
        }
        true
    }
}

/// Installs [`MediaPreviewState`] into Freya context for the current window.
pub fn use_provide_media_preview_state() -> MediaPreviewState {
    let open = use_state(|| false);
    let current_id = use_state(|| None::<LibraryMediaId>);
    use_provide_context(|| MediaPreviewState { open, current_id })
}

/// Consumes [`MediaPreviewState`] if installed in this window.
pub fn try_use_media_preview_state() -> Option<MediaPreviewState> {
    try_consume_context::<MediaPreviewState>()
}

/// Full-window media preview overlay drawn at [`POPUP_OVERLAY_LEVEL`].
#[derive(Clone, PartialEq)]
pub struct MediaPreviewOverlay;

impl Component for MediaPreviewOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let preview_state = try_use_media_preview_state();
        let menu_state = try_use_menu_state();

        let is_open = preview_state.is_some_and(|s| s.is_open());
        let current_id = preview_state.and_then(|s| *s.current_id.read());
        let anim = use_animation_with_dependencies(&is_open, move |conf, open| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *open { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(PREVIEW_MOTION)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let progress = anim.get().value().clamp(0.0, 1.0);
        if !is_open && progress <= 0.01 {
            return rect().into_element();
        }

        let items = core.model().read().library_media.clone();
        let current_index = current_id
            .and_then(|id| items.iter().position(|item| item.id == id))
            .or_else(|| (!items.is_empty()).then_some(0));
        let Some(index) = current_index else {
            return rect().into_element();
        };
        let item = items[index].clone();
        let total = items.len();
        let has_prev = index > 0;
        let has_next = index + 1 < total;

        let root_size = *Platform::get().current_window().root_size.read();
        let panel_width = (root_size.width * 0.72).clamp(520.0, 960.0);
        let stage_height = (root_size.height * 0.56).clamp(300.0, 540.0);

        let backdrop_color = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb((125.0 * progress).round() as u8, 0, 0, 0),
            ResolvedScheme::Light => Color::from_argb((148.0 * progress).round() as u8, 255, 255, 255),
        };
        let panel_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_rgb(28, 30, 34),
            ResolvedScheme::Light => Color::from_rgb(245, 246, 249),
        };
        let stage_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(80, 0, 0, 0),
            ResolvedScheme::Light => Color::from_argb(14, 0, 0, 0),
        };
        let hairline = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(22, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(22, 0, 0, 0),
        };
        let open_pill_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(30, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(24, 0, 0, 0),
        };

        let close_core = core.clone();
        let key_core = core.clone();
        let prev_core = core.clone();
        let next_core = core.clone();
        let open_core = core.clone();
        let show_core = core.clone();
        let copy_core = core.clone();
        let stage_prev_core = core.clone();
        let stage_next_core = core.clone();
        let media_id = item.id;

        let on_global_key_down = move |e: Event<KeyboardEventData>| {
            if !is_open {
                return;
            }
            match &e.key {
                Key::Named(NamedKey::Escape) => {
                    e.stop_propagation();
                    key_core.invoke_or_log(Command::LibraryMediaPreviewClose);
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    e.stop_propagation();
                    key_core.invoke_or_log(Command::LibraryMediaPreviewStep { delta: -1 });
                }
                Key::Named(NamedKey::ArrowRight) => {
                    e.stop_propagation();
                    key_core.invoke_or_log(Command::LibraryMediaPreviewStep { delta: 1 });
                }
                Key::Character(ch) if ch == "q" && e.modifiers.is_empty() => {
                    e.stop_propagation();
                    key_core.invoke_or_log(Command::LibraryMediaPreviewClose);
                }
                Key::Character(ch) if ch == "h" && e.modifiers.is_empty() => {
                    e.stop_propagation();
                    key_core.invoke_or_log(Command::LibraryMediaPreviewStep { delta: -1 });
                }
                Key::Character(ch) if ch == "l" && e.modifiers.is_empty() => {
                    e.stop_propagation();
                    key_core.invoke_or_log(Command::LibraryMediaPreviewStep { delta: 1 });
                }
                _ => {}
            }
        };

        let now_unix = current_unix_seconds();
        let relative_date = format_relative_date(item.modified_at_unix, now_unix);
        let opener_label = format!("Open in {}", default_opener_name(item.kind));
        let kind_badge = match item.kind {
            LibraryMediaKind::Image => format!("{} Image", item.extension.to_ascii_uppercase()),
            LibraryMediaKind::Video => format!("{} Video", item.extension.to_ascii_uppercase()),
            LibraryMediaKind::File => format!("{} File", item.extension.to_ascii_uppercase()),
        };

        let header = rect()
            .width(Size::fill())
            .padding(Gaps::new(10.0, 14.0, 10.0, 12.0))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .child(
                IconButton::new("close", "Close preview")
                    .size(28.0)
                    .icon_size(14.0)
                    .on_press({
                        let c = core.clone();
                        move |_| {
                            c.invoke_or_log(Command::LibraryMediaPreviewClose);
                        }
                    }),
            )
            .child(
                IconButton::new("arrow-left", "Previous media")
                    .size(28.0)
                    .icon_size(14.0)
                    .disabled(!has_prev)
                    .on_press(move |_| {
                        prev_core.invoke_or_log(Command::LibraryMediaPreviewStep { delta: -1 });
                    }),
            )
            .child(
                IconButton::new("arrow-right", "Next media")
                    .size(28.0)
                    .icon_size(14.0)
                    .disabled(!has_next)
                    .on_press(move |_| {
                        next_core.invoke_or_log(Command::LibraryMediaPreviewStep { delta: 1 });
                    }),
            )
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .overflow(Overflow::Clip)
                    .padding(Gaps::new(0.0, 4.0, 0.0, 4.0))
                    .child(
                        label()
                            .text(item.name.clone())
                            .font_size(13.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis)
                            .color(theme.color(ThemeRole::Text)),
                    )
                    .child(
                        label()
                            .text(relative_date)
                            .font_size(13.0)
                            .max_lines(1)
                            .color(theme.color(ThemeRole::TextMuted)),
                    )
                    .child(
                        label()
                            .text(format!("{} / {}", index + 1, total))
                            .font_size(12.0)
                            .max_lines(1)
                            .color(theme.color(ThemeRole::TextMuted)),
                    ),
            )
            .child({
                let menu_item = item.clone();
                IconButton::new("menu", "More options")
                    .size(28.0)
                    .icon_size(14.0)
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        if let Some(ms) = menu_state {
                            let pos = match e.data() {
                                PressEventData::Mouse(m) => (m.global_location.x as f32, m.global_location.y as f32),
                                _ => (root_size.width * 0.65, 160.0),
                            };
                            ms.open_context_menu(OpenContextMenu {
                                position: pos,
                                items: build_media_context_menu(&menu_item),
                                tab_id: None,
                                open_submenu: None,
                            });
                        }
                    })
            })
            .child(
                rect()
                    .padding(Gaps::new(6.0, 12.0, 6.0, 12.0))
                    .corner_radius(CornerRadius::new_all(8.0))
                    .background(open_pill_bg)
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |_| {
                        open_core.invoke_or_log(Command::LibraryMediaOpen { media_id });
                    })
                    .child(
                        label()
                            .text(opener_label)
                            .font_size(12.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .max_lines(1)
                            .color(theme.color(ThemeRole::Text)),
                    ),
            );

        let stage_media = render_stage_media(&item, panel_width - 96.0, stage_height - 40.0, &theme);
        let stage_ctx_item = item.clone();
        let stage = rect()
            .width(Size::fill())
            .height(Size::px(stage_height))
            .background(stage_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(hairline),
            )
            .center()
            .on_mouse_down(move |e: Event<MouseEventData>| {
                if e.button == Some(MouseButton::Right) {
                    e.stop_propagation();
                    if let Some(ms) = menu_state {
                        ms.open_context_menu(OpenContextMenu {
                            position: (e.global_location.x as f32, e.global_location.y as f32),
                            items: build_media_context_menu(&stage_ctx_item),
                            tab_id: None,
                            open_submenu: None,
                        });
                    }
                }
            })
            .child(stage_media)
            .maybe_child(has_prev.then(|| {
                rect()
                    .position(Position::new_absolute().left(14.0).top((stage_height - 38.0) * 0.5))
                    .width(Size::px(38.0))
                    .height(Size::px(38.0))
                    .corner_radius(CornerRadius::new_all(19.0))
                    .background(Color::from_argb(140, 0, 0, 0))
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        stage_prev_core.invoke_or_log(Command::LibraryMediaPreviewStep { delta: -1 });
                    })
                    .child(white_icon("arrow-left", 16.0))
            }))
            .maybe_child(has_next.then(|| {
                rect()
                    .position(Position::new_absolute().right(14.0).top((stage_height - 38.0) * 0.5))
                    .width(Size::px(38.0))
                    .height(Size::px(38.0))
                    .corner_radius(CornerRadius::new_all(19.0))
                    .background(Color::from_argb(140, 0, 0, 0))
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        stage_next_core.invoke_or_log(Command::LibraryMediaPreviewStep { delta: 1 });
                    })
                    .child(white_icon("arrow-right", 16.0))
            }));

        let mut detail_parts = vec![kind_badge, format_bytes(item.size_bytes)];
        if let Some(dims) = format_dimensions(item.dimensions) {
            detail_parts.push(dims);
        }
        if let Some(secs) = item.duration_secs {
            detail_parts.push(format_duration(secs));
        }
        if !item.folder.is_empty() {
            detail_parts.push(item.folder.clone());
        }

        let footer = rect()
            .width(Size::fill())
            .padding(Gaps::new(9.0, 14.0, 9.0, 14.0))
            .direction(Direction::Horizontal)
            .main_align(Alignment::SpaceBetween)
            .cross_align(Alignment::center())
            .spacing(12.0)
            .child(
                label()
                    .text(detail_parts.join(" · "))
                    .font_size(12.0)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .color(theme.color(ThemeRole::TextMuted)),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .child(
                        rect()
                            .padding(Gaps::new(4.0, 9.0, 4.0, 9.0))
                            .corner_radius(CornerRadius::new_all(7.0))
                            .background(open_pill_bg)
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(5.0)
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |_| {
                                show_core.invoke_or_log(Command::LibraryMediaShowInFolder { media_id });
                            })
                            .child(Icon::new("folder").size(13.0).role(ThemeRole::TextMuted))
                            .child(
                                label()
                                    .text("Show in Folder")
                                    .font_size(12.0)
                                    .color(theme.color(ThemeRole::Text)),
                            ),
                    )
                    .child(
                        rect()
                            .padding(Gaps::new(4.0, 9.0, 4.0, 9.0))
                            .corner_radius(CornerRadius::new_all(7.0))
                            .background(open_pill_bg)
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(5.0)
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |_| {
                                copy_core.invoke_or_log(Command::LibraryMediaCopy { media_id });
                            })
                            .child(Icon::new("copy").size(13.0).role(ThemeRole::TextMuted))
                            .child(label().text("Copy").font_size(12.0).color(theme.color(ThemeRole::Text))),
                    ),
            );

        let panel_offset_y = (1.0 - progress) * 14.0;
        let panel_shadow =
            Shadow::new()
                .x(0.0)
                .y(24.0)
                .blur(60.0)
                .color(Color::from_argb((125.0 * progress).round() as u8, 0, 0, 0));

        let panel = rect()
            .width(Size::px(panel_width))
            .corner_radius(CornerRadius::new_all(18.0))
            .overflow(Overflow::Clip)
            .background(panel_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(hairline),
            )
            .shadow(panel_shadow)
            .opacity(progress)
            .offset_y(panel_offset_y)
            .on_mouse_down(|e: Event<MouseEventData>| {
                e.stop_propagation();
            })
            .child(header)
            .child(stage)
            .child(footer);

        rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .on_global_key_down(on_global_key_down)
            .child(
                rect()
                    .position(Position::new_global().left(0.0).top(0.0))
                    .width(Size::window_percent(100.0))
                    .height(Size::window_percent(100.0))
                    .background(backdrop_color)
                    .on_mouse_down(move |_| {
                        close_core.invoke_or_log(Command::LibraryMediaPreviewClose);
                    }),
            )
            .child(rect().width(Size::fill()).height(Size::fill()).center().child(panel))
            .into_element()
    }
}

fn render_stage_media(
    item: &LibraryMediaItem,
    max_w: f32,
    max_h: f32,
    theme: &crate::ui::theme::ResolvedTheme,
) -> Element {
    let (fit_w, fit_h) = fit_dimensions(item.dimensions, max_w, max_h);

    match item.kind {
        LibraryMediaKind::Image => {
            let path = PathBuf::from(&item.path);
            if path.is_file() {
                rect()
                    .width(Size::px(fit_w))
                    .height(Size::px(fit_h))
                    .corner_radius(CornerRadius::new_all(8.0))
                    .overflow(Overflow::Clip)
                    .shadow(Shadow::new().y(8.0).blur(24.0).color(Color::from_argb(72, 0, 0, 0)))
                    .child(
                        ImageViewer::new(path)
                            .width(Size::px(fit_w))
                            .height(Size::px(fit_h))
                            .aspect_ratio(AspectRatio::Min)
                            .image_cover(ImageCover::Center),
                    )
                    .into_element()
            } else {
                rect()
                    .width(Size::px(220.0))
                    .height(Size::px(160.0))
                    .corner_radius(CornerRadius::new_all(12.0))
                    .background(theme.color(ThemeRole::SurfaceRaised))
                    .center()
                    .spacing(8.0)
                    .child(Icon::new("camera").size(36.0).role(ThemeRole::TextMuted))
                    .child(
                        label()
                            .text("Image file unavailable")
                            .font_size(12.5)
                            .color(theme.color(ThemeRole::TextMuted)),
                    )
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
                    .width(Size::px(fit_w))
                    .height(Size::px(fit_h))
                    .aspect_ratio(AspectRatio::Max)
                    .image_cover(ImageCover::Center)
                    .into_element()
            } else {
                rect()
                    .width(Size::px(fit_w))
                    .height(Size::px(fit_h))
                    .background(Color::from_rgb(20, 24, 34))
                    .into_element()
            };

            let (progress_pct, time_label) = match item.duration_secs {
                Some(total) if total > 0 => {
                    let elapsed = (total / 4).min(total);
                    (
                        (elapsed as f32 / total as f32 * 100.0).clamp(0.0, 100.0),
                        format!("{} / {}", format_duration(elapsed), format_duration(total)),
                    )
                }
                _ => (0.0, "0:00".to_string()),
            };

            rect()
                .width(Size::px(fit_w))
                .height(Size::px(fit_h))
                .corner_radius(CornerRadius::new_all(10.0))
                .overflow(Overflow::Clip)
                .shadow(Shadow::new().y(8.0).blur(24.0).color(Color::from_argb(86, 0, 0, 0)))
                .child(backdrop)
                .child(
                    rect()
                        .position(
                            Position::new_absolute()
                                .top((fit_h - 64.0) * 0.5)
                                .left((fit_w - 64.0) * 0.5),
                        )
                        .width(Size::px(64.0))
                        .height(Size::px(64.0))
                        .corner_radius(CornerRadius::new_all(32.0))
                        .background(Color::from_argb(165, 0, 0, 0))
                        .center()
                        .child(white_icon("media-play", 26.0)),
                )
                .child(
                    rect()
                        .position(Position::new_absolute().bottom(12.0).left(14.0))
                        .width(Size::px((fit_w - 28.0).max(160.0)))
                        .height(Size::px(34.0))
                        .padding(Gaps::new(0.0, 12.0, 0.0, 12.0))
                        .corner_radius(CornerRadius::new_all(10.0))
                        .background(Color::from_argb(185, 16, 18, 24))
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .cross_align(Alignment::center())
                        .spacing(10.0)
                        .child(white_icon("media-play", 13.0))
                        .child(
                            rect()
                                .width(Size::flex(1.0))
                                .height(Size::px(4.0))
                                .corner_radius(CornerRadius::new_all(2.0))
                                .background(Color::from_argb(55, 255, 255, 255))
                                .child(
                                    rect()
                                        .width(Size::percent(progress_pct))
                                        .height(Size::fill())
                                        .corner_radius(CornerRadius::new_all(2.0))
                                        .background(theme.color(ThemeRole::Accent)),
                                ),
                        )
                        .child(
                            label()
                                .text(time_label)
                                .font_size(11.5)
                                .max_lines(1)
                                .color(Color::WHITE),
                        ),
                )
                .into_element()
        }
        LibraryMediaKind::File => rect()
            .center()
            .spacing(10.0)
            .child(Icon::new("page-portrait").size(72.0).role(ThemeRole::TextMuted))
            .child(
                label()
                    .text(format!(".{} preview unavailable", item.extension))
                    .font_size(13.0)
                    .color(theme.color(ThemeRole::TextMuted)),
            )
            .into_element(),
    }
}

fn white_icon(name: &str, size: f32) -> Element {
    if let Some((canonical, bytes)) = prepared_icon_bytes(name) {
        SvgViewer::new((("zendia-icon-white", canonical), bytes))
            .width(Size::px(size))
            .height(Size::px(size))
            .color(Color::WHITE)
            .show_loader(false)
            .into_element()
    } else {
        Icon::new(name).size(size).role(ThemeRole::Text).into_element()
    }
}

fn fit_dimensions(dimensions: Option<(u32, u32)>, max_w: f32, max_h: f32) -> (f32, f32) {
    let (w, h) = dimensions
        .map(|(w, h)| (w.max(1) as f32, h.max(1) as f32))
        .unwrap_or((640.0, 400.0));
    let scale = (max_w / w).min(max_h / h).min(2.2);
    ((w * scale).clamp(180.0, max_w), (h * scale).clamp(140.0, max_h))
}
