/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Centered modal dialog for WebExtension install, permission, and removal consent prompts.
//!
//! An install prompt leads with the extension itself (icon, name, version, where it comes from) and lists what it will
//! be able to do, with broad access (every site, browsing history, the debugger) marked as a warning. The dialog rises
//! in on the shared popup spring with the backdrop fading behind it, and fades out once answered.

use crate::ui::core_view::use_core;
use crate::ui::extensions::ExtensionIconView;
use crate::ui::kit::{GlassPanel, Icon, PopupPresence, use_interactive_feedback, use_last_some};
use crate::ui::theme::{CONSENT_OVERLAY_LEVEL, ResolvedTheme, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::modules::{ExtensionIcon, ExtensionPrompt, ExtensionPromptDetail, ExtensionPromptSubject};

/// How dark the page gets behind a consent prompt.
const BACKDROP_ALPHA: u8 = 110;

/// Window-root overlay showing any pending [`ExtensionPrompt`](zendia_browser_core::modules::ExtensionPrompt) from
/// `ExtensionServer`.
#[derive(Clone, PartialEq)]
pub struct ExtensionPromptOverlay;

impl Component for ExtensionPromptOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let pending = core.model().read().pending_extension_prompt.clone();
        let is_open = pending.is_some();
        // The answered prompt stays drawn while it fades out.
        let shown = use_last_some(&pending);

        let Some(prompt) = shown else {
            return rect().into_element();
        };

        let token = prompt.token;
        let core_cancel = core.clone();
        let core_accept = core.clone();
        let core_esc = core.clone();
        let has_high_risk = prompt.details.iter().any(|d| d.high_risk);

        let card = GlassPanel::new()
            .width(Size::px(440.0))
            .padding(Gaps::new_all(theme.spacing.xxl + 4.0))
            .spacing(theme.spacing.xxl)
            .child(header(&prompt, &theme))
            .maybe_child(details(&prompt, &theme))
            .maybe_child(has_high_risk.then(|| {
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::Center)
                    .spacing(theme.spacing.sm)
                    .child(Icon::new("security").size(12.0).role(ThemeRole::TextMuted))
                    .child(
                        label()
                            .width(Size::flex(1.0))
                            .text("Only add extensions from developers you trust.")
                            .font_size(theme.font_sizes.sm)
                            .color(theme.color(ThemeRole::TextMuted)),
                    )
            }))
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::End)
                    .spacing(theme.spacing.md)
                    .child(
                        DialogButton {
                            label: "Cancel".to_string(),
                            primary: false,
                        }
                        .on_press(move || {
                            core_cancel.invoke_or_log(Command::ExtensionPromptAnswer { token, accepted: false })
                        }),
                    )
                    .child(
                        DialogButton {
                            label: prompt.accept_label.clone(),
                            primary: true,
                        }
                        .on_press(move || {
                            core_accept.invoke_or_log(Command::ExtensionPromptAnswer { token, accepted: true })
                        }),
                    ),
            );

        PopupPresence::centered(is_open)
            .open_seq(token)
            .layer(Layer::OverlayLevel(CONSENT_OVERLAY_LEVEL))
            .backdrop_color(Color::from_argb(BACKDROP_ALPHA, 0, 0, 0))
            // A click outside does not answer: the user has to choose.
            .on_backdrop_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .on_escape(move |_| {
                core_esc.invoke_or_log(Command::ExtensionPromptAnswer { token, accepted: false });
            })
            .child(
                rect()
                    .a11y_role(AccessibilityRole::AlertDialog)
                    .a11y_alt(prompt.title.clone())
                    .max_width(Size::percent(90.0))
                    .child(card),
            )
            .into_element()
    }
}

fn header(prompt: &ExtensionPrompt, theme: &ResolvedTheme) -> Element {
    let Some(subject) = &prompt.subject else {
        // Permission and removal prompts: a plain title and its message.
        return rect()
            .width(Size::fill())
            .spacing(theme.spacing.lg)
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::Center)
                    .spacing(theme.spacing.lg)
                    .child(ExtensionTile {
                        cache_key: format!("prompt-{}", prompt.token),
                        name: String::new(),
                        icon: None,
                        size: 36.0,
                    })
                    .child(
                        label()
                            .width(Size::flex(1.0))
                            .text(prompt.title.clone())
                            .font_size(theme.font_sizes.xl + 1.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(theme.color(ThemeRole::Text)),
                    ),
            )
            .maybe_child(message(prompt, theme))
            .into_element();
    };

    rect()
        .width(Size::fill())
        .spacing(theme.spacing.xl)
        .child(
            rect()
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .cross_align(Alignment::Center)
                .spacing(theme.spacing.xl + 2.0)
                .child(ExtensionTile {
                    cache_key: format!("prompt-{}", prompt.token),
                    name: subject.name.clone(),
                    icon: subject.icon.clone(),
                    size: 52.0,
                })
                .child(
                    rect()
                        .width(Size::flex(1.0))
                        .spacing(theme.spacing.xs)
                        .child(
                            label()
                                .width(Size::fill())
                                .text(prompt.title.clone())
                                .font_size(theme.font_sizes.title)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .max_lines(2)
                                .text_overflow(TextOverflow::Ellipsis)
                                .color(theme.color(ThemeRole::Text)),
                        )
                        .child(SubjectMeta {
                            subject: subject.clone(),
                        }),
                ),
        )
        .maybe_child(message(prompt, theme))
        .into_element()
}

fn message(prompt: &ExtensionPrompt, theme: &ResolvedTheme) -> Option<Element> {
    (!prompt.message.is_empty()).then(|| {
        label()
            .width(Size::fill())
            .text(prompt.message.clone())
            .font_size(theme.font_sizes.md)
            .color(theme.color(ThemeRole::TextMuted))
            .into_element()
    })
}

/// "Version 4.9" and the source as a small accent pill.
#[derive(Clone, PartialEq)]
struct SubjectMeta {
    subject: ExtensionPromptSubject,
}

impl Component for SubjectMeta {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let muted = theme.color(ThemeRole::TextMuted);
        rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .spacing(theme.spacing.sm)
            .maybe_child((!self.subject.version.is_empty()).then(|| {
                label()
                    .text(format!("Version {}", self.subject.version))
                    .font_size(theme.font_sizes.sm)
                    .color(muted)
            }))
            .maybe_child((!self.subject.source.is_empty()).then(|| {
                rect()
                    .padding(Gaps::new(2.0, 7.0, 2.0, 7.0))
                    .corner_radius(CornerRadius::new_all(theme.radii.pill))
                    .background(theme.rgba(ThemeRole::Accent).with_alpha(0.12).to_color())
                    .child(
                        label()
                            .text(self.subject.source.clone())
                            .font_size(theme.font_sizes.xs)
                            .font_weight(FontWeight::MEDIUM)
                            .color(theme.color(ThemeRole::Accent)),
                    )
            }))
    }
}

fn details(prompt: &ExtensionPrompt, theme: &ResolvedTheme) -> Option<Element> {
    if prompt.details.is_empty() {
        return None;
    }
    // Broad access first, so it is read before the rest.
    let mut ordered: Vec<&ExtensionPromptDetail> = prompt.details.iter().filter(|d| d.high_risk).collect();
    ordered.extend(prompt.details.iter().filter(|d| !d.high_risk));

    let rows: Vec<Element> = ordered.into_iter().map(|detail| detail_row(detail, theme)).collect();

    // A long list scrolls in a box of fixed height; a short one takes the height it needs.
    const ROWS_WITHOUT_SCROLLING: usize = 6;
    let list: Element = if rows.len() > ROWS_WITHOUT_SCROLLING {
        ScrollView::new()
            .width(Size::fill())
            .height(Size::px(232.0))
            .spacing(theme.spacing.xs)
            .children(rows)
            .into_element()
    } else {
        rect()
            .width(Size::fill())
            .spacing(theme.spacing.xs)
            .children(rows)
            .into_element()
    };
    Some(list)
}

fn detail_row(detail: &ExtensionPromptDetail, theme: &ResolvedTheme) -> Element {
    let (icon, icon_role, background, border) = if detail.high_risk {
        let warning = theme.rgba(ThemeRole::Warning);
        (
            "warning",
            ThemeRole::Warning,
            warning.with_alpha(0.10).to_color(),
            warning.with_alpha(0.35).to_color(),
        )
    } else {
        (
            "check",
            ThemeRole::Success,
            theme.rgba(ThemeRole::SurfaceRaised).with_alpha(0.7).to_color(),
            Color::TRANSPARENT,
        )
    };
    rect()
        .width(Size::fill())
        .padding(Gaps::new(9.0, 12.0, 9.0, 10.0))
        .corner_radius(CornerRadius::new_all(theme.radii.md + 2.0))
        .background(background)
        .border(Border::new().width(1.0).alignment(BorderAlignment::Inner).fill(border))
        .direction(Direction::Horizontal)
        .content(Content::Flex)
        .cross_align(Alignment::Center)
        .spacing(theme.spacing.lg)
        .child(
            rect()
                .width(Size::px(22.0))
                .height(Size::px(22.0))
                .corner_radius(CornerRadius::new_all(theme.radii.pill))
                .background(theme.rgba(icon_role).with_alpha(0.16).to_color())
                .center()
                .child(Icon::new(icon).size(12.0).role(icon_role)),
        )
        .child(
            label()
                .width(Size::flex(1.0))
                .text(detail.text.clone())
                .font_size(theme.font_sizes.md)
                .font_weight(if detail.high_risk {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .color(theme.color(ThemeRole::Text)),
        )
        .into_element()
}

/// An extension's icon in a rounded tile, or the first letter of its name until the icon is known.
#[derive(Clone, PartialEq)]
pub struct ExtensionTile {
    pub cache_key: String,
    pub name: String,
    pub icon: Option<ExtensionIcon>,
    pub size: f32,
}

impl Component for ExtensionTile {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let size = self.size;
        let accent = theme.rgba(ThemeRole::Accent);
        let tile = rect()
            .width(Size::px(size))
            .height(Size::px(size))
            .corner_radius(CornerRadius::new_all((size * 0.26).round()))
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .shadow(
                Shadow::new()
                    .x(0.0)
                    .y(size * 0.06)
                    .blur(size * 0.3)
                    .spread(-size * 0.12)
                    .color(accent.with_alpha(0.35).to_color()),
            )
            .center();
        if self.icon.is_some() {
            return tile
                .background(theme.color(ThemeRole::Surface))
                .child(ExtensionIconView::new(
                    self.cache_key.clone(),
                    "extension",
                    self.icon.clone(),
                    (size * 0.68).round(),
                    ThemeRole::Accent,
                ))
                .into_element();
        }
        let initial = self
            .name
            .chars()
            .find(|c| c.is_alphanumeric())
            .map(|c| c.to_uppercase().collect::<String>())
            .unwrap_or_default();
        let gradient = LinearGradient::new()
            .angle(135.0)
            .stop((accent.with_alpha(0.28).to_color(), 0.0))
            .stop((accent.with_alpha(0.10).to_color(), 100.0));
        let tile = tile.background(gradient);
        if initial.is_empty() {
            return tile
                .child(Icon::new("extension").size(size * 0.45).role(ThemeRole::Accent))
                .into_element();
        }
        tile.child(
            label()
                .text(initial)
                .font_size(size * 0.42)
                .font_weight(FontWeight::BOLD)
                .color(theme.color(ThemeRole::Accent)),
        )
        .into_element()
    }
}

/// A dialog or card button: the accent fill for the main action, a quiet raised one otherwise. Hover fades in and a
/// press shrinks it slightly, as the kit's other buttons do.
#[derive(Clone, PartialEq)]
pub struct DialogButton {
    pub label: String,
    pub primary: bool,
}

impl DialogButton {
    /// `handler`'s result (such as whether a command ran) is not used.
    pub fn on_press<R>(self, mut handler: impl FnMut() -> R + 'static) -> DialogButtonWithHandler {
        DialogButtonWithHandler {
            button: self,
            handler: EventHandler::new(move |_: ()| {
                handler();
            }),
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct DialogButtonWithHandler {
    button: DialogButton,
    handler: EventHandler<()>,
}

impl Component for DialogButtonWithHandler {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let feedback = use_interactive_feedback(true);
        let handler = self.handler.clone();
        let primary = self.button.primary;
        let accent = theme.rgba(ThemeRole::Accent);
        let raised = theme.rgba(ThemeRole::SurfaceRaised);
        let text = theme.rgba(ThemeRole::Text);
        let background = if primary {
            feedback.background(
                accent.to_color(),
                accent.mix(text, 0.10).to_color(),
                accent.mix(text, 0.18).to_color(),
            )
        } else {
            feedback.background(
                raised.with_alpha(0.8).to_color(),
                theme.color(ThemeRole::Hover),
                raised.mix(text, 0.10).to_color(),
            )
        };
        let mut button = rect()
            .padding(Gaps::new(8.0, 16.0, 8.0, 16.0))
            .corner_radius(CornerRadius::new_all(theme.radii.lg))
            .background(background)
            .scale(feedback.scale())
            .cursor(CursorIcon::Pointer)
            .a11y_role(AccessibilityRole::Button)
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .on_pointer_down(feedback.on_pointer_down())
            .on_global_pointer_up(feedback.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                handler.call(());
            });
        button = if primary {
            button.shadow(
                Shadow::new()
                    .x(0.0)
                    .y(3.0)
                    .blur(10.0)
                    .spread(-4.0)
                    .color(accent.with_alpha(0.55).to_color()),
            )
        } else {
            button.border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
        };
        button.child(
            label()
                .text(self.button.label.clone())
                .font_size(theme.font_sizes.md)
                .font_weight(if primary {
                    FontWeight::SEMI_BOLD
                } else {
                    FontWeight::MEDIUM
                })
                .color(theme.color(if primary {
                    ThemeRole::TextOnAccent
                } else {
                    ThemeRole::Text
                })),
        )
    }
}
