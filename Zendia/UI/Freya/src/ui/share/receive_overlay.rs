/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Incoming share link preview and import overlay (`.zen-share-overlay`).
//!
//! Shown over the active browser tab when navigating to a Zendia share URL, previewing the shared folders, split
//! groups and tabs with a stacked badge footer and a primary accent button to import the space or folder into Zendia.

use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, focus_ring_border};
use crate::ui::theme::{ResolvedScheme, Rgba, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::TabId;
use zendia_browser_core::share::{SharePreview, SharePreviewItem, SharePreviewKind};

const OVERLAY_ENTER: Duration = Duration::from_millis(350);
const CARD_WIDTH: f32 = 325.0;

/// Full-page overlay showing an incoming [`SharePreview`] over the active tab.
#[derive(Clone, PartialEq)]
pub struct ShareReceiveOverlay {
    pub preview: SharePreview,
    pub host_tab_id: Option<TabId>,
}

impl Component for ShareReceiveOverlay {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let enter_anim = use_animation_with_dependencies(&self.preview.share_id, move |conf, _| {
            conf.on_creation(OnCreation::Run);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(OVERLAY_ENTER)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let t = enter_anim.get().value().clamp(0.0, 1.0);
        let bg = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(18, 18, 22, 0.92).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(244, 245, 248, 0.92).to_color(),
        };

        rect()
            .layer(Layer::Relative(18))
            .position(Position::new_absolute().left(0.0).top(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .background(bg)
            .center()
            .padding(Gaps::new_all(32.0))
            .opacity(t)
            .offset_y(14.0 * (1.0 - t))
            .scale(0.98 + 0.02 * t)
            .child(ShareReceiveCard {
                preview: self.preview.clone(),
                host_tab_id: self.host_tab_id,
            })
    }
}

/// Reusable share preview card + primary import action button (also rendered inline in the UI gallery).
#[derive(Clone, PartialEq)]
pub struct ShareReceiveCard {
    pub preview: SharePreview,
    pub host_tab_id: Option<TabId>,
}

impl Component for ShareReceiveCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut importing = use_state(|| false);
        let mut add_hovered = use_state(|| false);
        let add_a11y = use_a11y();
        let add_focus = use_focus(add_a11y);

        let is_dark = theme.scheme == ResolvedScheme::Dark;
        let card_bg = if is_dark {
            Color::from_rgb(30, 30, 35)
        } else {
            Color::WHITE
        };
        let badge_bg = if is_dark {
            Color::from_rgb(28, 28, 30)
        } else {
            Color::WHITE
        };
        let sheet1_bg = if is_dark {
            Color::from_rgb(34, 34, 38)
        } else {
            Color::from_rgb(248, 248, 250)
        };
        let sheet2_bg = if is_dark {
            Color::from_rgb(40, 40, 45)
        } else {
            Color::from_rgb(240, 240, 244)
        };
        let footer_bg = theme
            .rgba(ThemeRole::Accent)
            .mix(
                if is_dark {
                    Rgba::from_rgb_alpha(30, 30, 35, 1.0)
                } else {
                    Rgba::from_rgb_alpha(255, 255, 255, 1.0)
                },
                0.92,
            )
            .to_color();
        let border_color = theme.rgba(ThemeRole::BorderSubtle).to_color();
        let primary_text = theme.color(ThemeRole::Text);
        let muted_text = theme.color(ThemeRole::TextMuted);

        let subtitle = match &self.preview.sharer_name {
            Some(name) if !name.trim().is_empty() => match self.preview.kind {
                SharePreviewKind::Space => format!("Space from {name}"),
                SharePreviewKind::Folder => format!("Folder from {name}"),
            },
            _ => "Shared with you".to_string(),
        };
        let add_label = match self.preview.kind {
            SharePreviewKind::Space => "Add Space to Zendia",
            SharePreviewKind::Folder => "Add Folder to Zendia",
        };
        let badge_icon = match self.preview.kind {
            SharePreviewKind::Space => "duplicate-tab",
            SharePreviewKind::Folder => "folder",
        };

        let items: Vec<Element> = self
            .preview
            .items
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                SharePreviewItemView {
                    item: item.clone(),
                    key: DiffKey::from(&format!("share-item-{idx}")),
                }
                .into_element()
            })
            .collect();

        let preview_clone = self.preview.clone();
        let host_tab_id = self.host_tab_id;
        let run_import = {
            let core = core.clone();
            move || {
                if *importing.peek() {
                    return;
                }
                importing.set(true);
                match preview_clone.kind {
                    SharePreviewKind::Space => {
                        core.invoke_or_log(Command::SpaceCreate {
                            name: Some(preview_clone.name.clone()),
                            icon: Some("sparkles".to_string()),
                        });
                    }
                    SharePreviewKind::Folder => {
                        core.invoke_or_log(Command::FolderCreate {
                            name: Some(preview_clone.name.clone()),
                            space_id: None,
                            tab_id: None,
                        });
                    }
                }
                for (url, _) in collect_preview_urls(&preview_clone.items) {
                    core.invoke_or_log(Command::TabNew {
                        url: Some(url),
                        after: None,
                        activate: false,
                    });
                }
                core.invoke_or_log(Command::ToastShow {
                    message: format!("Imported \"{}\"", preview_clone.name),
                    timeout_ms: Some(3000),
                    action_label: None,
                    action_command: None,
                });
                if let Some(tab_id) = host_tab_id {
                    core.invoke_or_log(Command::TabClose {
                        tab_id: Some(tab_id),
                        force: true,
                    });
                }
            }
        };
        let mut click_import = run_import.clone();
        let mut key_import = run_import;

        let accent_bg = if *add_hovered.read() {
            theme
                .rgba(ThemeRole::Accent)
                .mix(Rgba::from_rgb_alpha(255, 255, 255, 1.0), 0.12)
                .to_color()
        } else {
            theme.color(ThemeRole::Accent)
        };

        let badge_stack = rect()
            .width(Size::fill())
            .padding(Gaps::new(14.0, 0.0, 0.0, 0.0))
            .child(
                rect()
                    .position(Position::new_absolute().top(0.0).left(22.0))
                    .width(Size::px(CARD_WIDTH - 36.0 - 44.0))
                    .height(Size::px(24.0))
                    .corner_radius(CornerRadius::new_all(14.0))
                    .background(sheet2_bg)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(border_color),
                    ),
            )
            .child(
                rect()
                    .position(Position::new_absolute().top(6.0).left(10.0))
                    .width(Size::px(CARD_WIDTH - 36.0 - 20.0))
                    .height(Size::px(24.0))
                    .corner_radius(CornerRadius::new_all(14.0))
                    .background(sheet1_bg)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(border_color),
                    ),
            )
            .child(
                rect()
                    .layer(Layer::Relative(2))
                    .width(Size::fill())
                    .padding(Gaps::new(14.0, 18.0, 14.0, 18.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .spacing(14.0)
                    .corner_radius(CornerRadius::new_all(18.0))
                    .background(badge_bg)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(border_color),
                    )
                    .shadow(
                        Shadow::new()
                            .x(0.0)
                            .y(2.0)
                            .blur(10.0)
                            .color(Color::from_argb(24, 0, 0, 0)),
                    )
                    .child(Icon::new(badge_icon).size(24.0).role(ThemeRole::Accent))
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .spacing(2.0)
                            .child(
                                label()
                                    .text(self.preview.name.clone())
                                    .font_size(14.0)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(primary_text)
                                    .max_lines(1)
                                    .text_overflow(TextOverflow::Ellipsis),
                            )
                            .child(
                                label()
                                    .text(subtitle)
                                    .font_size(12.0)
                                    .color(muted_text)
                                    .max_lines(1)
                                    .text_overflow(TextOverflow::Ellipsis),
                            ),
                    ),
            );

        rect()
            .cross_align(Alignment::center())
            .spacing(24.0)
            .child(
                rect()
                    .a11y_role(AccessibilityRole::Dialog)
                    .a11y_alt(format!("Shared preview: {}", self.preview.name))
                    .width(Size::px(CARD_WIDTH))
                    .max_height(Size::px(540.0))
                    .corner_radius(CornerRadius::new_all(28.0))
                    .overflow(Overflow::Clip)
                    .background(card_bg)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(border_color),
                    )
                    .shadow(Shadow::new().x(0.0).y(14.0).blur(40.0).color(Color::from_argb(
                        if is_dark { 120 } else { 52 },
                        0,
                        0,
                        0,
                    )))
                    .child(
                        ScrollView::new().width(Size::fill()).max_height(Size::px(340.0)).child(
                            rect()
                                .width(Size::fill())
                                .padding(Gaps::new(20.0, 18.0, 14.0, 18.0))
                                .spacing(6.0)
                                .children(items),
                        ),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::px(1.5))
                            .background(theme.rgba(ThemeRole::Accent).with_alpha(0.24).to_color()),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new(16.0, 18.0, 18.0, 18.0))
                            .background(footer_bg)
                            .child(badge_stack),
                    ),
            )
            .child(
                rect()
                    .a11y_id(add_a11y)
                    .a11y_focusable(true)
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt(add_label)
                    .min_width(Size::px(260.0))
                    .padding(Gaps::new(14.0, 36.0, 14.0, 36.0))
                    .corner_radius(CornerRadius::new_all(18.0))
                    .background(accent_bg)
                    .border(focus_ring_border(add_focus() == Focus::Keyboard, &theme, Border::new()))
                    .opacity(if *importing.read() { 0.5 } else { 1.0 })
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_pointer_enter(move |_| add_hovered.set_if_modified(true))
                    .on_pointer_leave(move |_| add_hovered.set_if_modified(false))
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        click_import();
                    })
                    .on_key_down(move |e: Event<KeyboardEventData>| {
                        if matches!(e.key, Key::Named(NamedKey::Enter)) || e.key == Key::Character(" ".into()) {
                            e.stop_propagation();
                            e.prevent_default();
                            key_import();
                        }
                    })
                    .child(
                        label()
                            .text(add_label)
                            .font_size(14.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(theme.color(ThemeRole::TextOnAccent)),
                    ),
            )
    }
}

#[derive(Clone, PartialEq)]
struct SharePreviewItemView {
    item: SharePreviewItem,
    key: DiffKey,
}

impl KeyExt for SharePreviewItemView {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for SharePreviewItemView {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut collapsed = use_state(|| false);
        let mut folder_hovered = use_state(|| false);

        match &self.item {
            SharePreviewItem::Tab { title, url, icon } => SharePreviewTabRow {
                title: title.clone(),
                url: url.clone(),
                icon: icon.clone(),
                compact: false,
            }
            .into_element(),
            SharePreviewItem::SplitView { tabs } => {
                let divider = theme.color(ThemeRole::BorderSubtle);
                let mut children = Vec::new();
                for (i, (title, url, icon)) in tabs.iter().enumerate() {
                    if i > 0 {
                        children.push(
                            rect()
                                .width(Size::px(1.0))
                                .height(Size::px(20.0))
                                .background(divider)
                                .into_element(),
                        );
                    }
                    children.push(
                        rect()
                            .width(Size::flex(1.0))
                            .child(SharePreviewTabRow {
                                title: title.clone(),
                                url: url.clone(),
                                icon: icon.clone(),
                                compact: true,
                            })
                            .into_element(),
                    );
                }
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .spacing(6.0)
                    .children(children)
                    .into_element()
            }
            SharePreviewItem::Folder { name, icon, items } => {
                let is_collapsed = *collapsed.read();
                let hover_bg = if *folder_hovered.read() {
                    theme.rgba(ThemeRole::Text).with_alpha(0.07).to_color()
                } else {
                    Color::TRANSPARENT
                };
                let child_els: Vec<Element> = items
                    .iter()
                    .enumerate()
                    .map(|(idx, child)| {
                        SharePreviewItemView {
                            item: child.clone(),
                            key: DiffKey::from(&format!("folder-child-{idx}")),
                        }
                        .into_element()
                    })
                    .collect();
                rect()
                    .width(Size::fill())
                    .spacing(4.0)
                    .child(
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new(8.0, 12.0, 8.0, 12.0))
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(12.0)
                            .corner_radius(CornerRadius::new_all(12.0))
                            .background(hover_bg)
                            .cursor(CursorIcon::Pointer)
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt(format!("Toggle folder {name}"))
                            .on_pointer_enter(move |_| folder_hovered.set_if_modified(true))
                            .on_pointer_leave(move |_| folder_hovered.set_if_modified(false))
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                let next = !*collapsed.peek();
                                collapsed.set(next);
                            })
                            .child(Icon::new(icon.clone()).size(18.0).role(ThemeRole::Accent))
                            .child(
                                label()
                                    .text(name.clone())
                                    .font_size(14.0)
                                    .font_weight(FontWeight::MEDIUM)
                                    .color(theme.color(ThemeRole::Text))
                                    .max_lines(1)
                                    .text_overflow(TextOverflow::Ellipsis),
                            ),
                    )
                    .maybe_child((!is_collapsed).then(|| {
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new(0.0, 0.0, 0.0, 24.0))
                            .spacing(4.0)
                            .children(child_els)
                    }))
                    .into_element()
            }
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct SharePreviewTabRow {
    title: String,
    url: String,
    icon: Option<String>,
    compact: bool,
}

impl Component for SharePreviewTabRow {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let url = self.url.clone();
        let bg = if *hovered.read() {
            theme.rgba(ThemeRole::Text).with_alpha(0.07).to_color()
        } else {
            Color::TRANSPARENT
        };

        rect()
            .width(Size::fill())
            .padding(if self.compact {
                Gaps::new(6.0, 10.0, 6.0, 10.0)
            } else {
                Gaps::new(9.0, 12.0, 9.0, 12.0)
            })
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .spacing(10.0)
            .corner_radius(CornerRadius::new_all(if self.compact { 8.0 } else { 12.0 }))
            .background(bg)
            .cursor(CursorIcon::Pointer)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.title.clone())
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                core.invoke_or_log(Command::GlanceOpen {
                    url: url.clone(),
                    parent_tab_id: None,
                    origin: None,
                });
            })
            .child(
                Icon::new(self.icon.clone().unwrap_or_else(|| "globe".to_string()))
                    .size(16.0)
                    .role(ThemeRole::TextMuted),
            )
            .child(
                label()
                    .width(Size::flex(1.0))
                    .text(self.title.clone())
                    .font_size(13.5)
                    .font_weight(FontWeight::MEDIUM)
                    .color(theme.color(ThemeRole::Text))
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
    }
}

fn collect_preview_urls(items: &[SharePreviewItem]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for item in items {
        match item {
            SharePreviewItem::Tab { title, url, .. } => out.push((url.clone(), title.clone())),
            SharePreviewItem::Folder { items, .. } => out.extend(collect_preview_urls(items)),
            SharePreviewItem::SplitView { tabs } => {
                for (title, url, _) in tabs {
                    out.push((url.clone(), title.clone()));
                }
            }
        }
    }
    out
}
