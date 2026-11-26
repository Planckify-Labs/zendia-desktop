/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Full-panel slide-in overlay for managing reusable agent instructions (`AgentInstructionsPage`).
//!
//! Matches Zendia's `ZendiaAgentInstructionsView` layout:
//! - Header: thin `‹` back chevron, `"Agent instructions"` title, `×` close button.
//! - Empty state: top-aligned helper paragraph + full-width pill outline `"Add instruction"` button.
//! - Populated state: compact scrollable guideline cards (accent border when enabled, `KitSwitch`, borderless bold
//!   title `Input`, trash icon, `72x72` dashed-style `+` file slot or file tile with `×`, subtle-fill multiline
//!   textarea) followed inline by a full-width `"+ Add instruction"` pill outline button.
//! - Footer: full-width solid `ThemeRole::Accent` `"Done"` pill button.

use crate::ui::agent::{agent_input_colors, chevron_left_icon};
use crate::ui::core_view::use_core;
use crate::ui::kit::{Icon, KitSwitch, TextField};
use crate::ui::panels::try_use_dock_state;
use crate::ui::theme::{ResolvedScheme, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::agent::{AgentInstruction, AttachmentKind, InstructionAttachment, format_file_size};
use zendia_browser_core::commands::Command;

/// Full-panel overlay editor for persistent agent instructions.
#[derive(Clone, PartialEq)]
pub struct AgentInstructionsPage {
    pub instructions: Vec<AgentInstruction>,
    pub accent: Color,
    key: DiffKey,
}

impl KeyExt for AgentInstructionsPage {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl AgentInstructionsPage {
    pub fn new(instructions: Vec<AgentInstruction>, accent: Color) -> Self {
        Self {
            instructions,
            accent,
            key: DiffKey::None,
        }
    }
}

impl Component for AgentInstructionsPage {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let dock_state = try_use_dock_state();

        let panel_bg = match theme.scheme {
            ResolvedScheme::Dark => theme.color(ThemeRole::Surface),
            ResolvedScheme::Light => theme.color(ThemeRole::SurfaceRaised),
        };
        let border_col = theme.color(ThemeRole::BorderSubtle);
        let accent_col = theme.color(ThemeRole::Accent);

        let core_add = core.clone();

        let header = rect()
            .width(Size::fill())
            .height(Size::px(42.0))
            .padding(Gaps::new(0.0, 12.0, 0.0, 12.0))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .main_align(Alignment::SpaceBetween)
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Back to conversation")
                            .width(Size::px(22.0))
                            .height(Size::px(22.0))
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                if let Some(mut ds) = dock_state {
                                    ds.instructions_open.set(false);
                                }
                            })
                            .child(chevron_left_icon(13.0, theme.color(ThemeRole::TextMuted))),
                    )
                    .child(
                        label()
                            .text("Agent instructions")
                            .font_size(13.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(theme.color(ThemeRole::Text)),
                    ),
            )
            .child(
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("Close instructions")
                    .width(Size::px(24.0))
                    .height(Size::px(24.0))
                    .corner_radius(CornerRadius::new_all(6.0))
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        if let Some(mut ds) = dock_state {
                            ds.instructions_open.set(false);
                        }
                    })
                    .child(Icon::new("close").size(12.5).role(ThemeRole::TextMuted)),
            );

        let add_btn = |label_text: &'static str, accent_outline: bool| {
            let core_a = core_add.clone();
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Add instruction")
                .width(Size::fill())
                .height(Size::px(36.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(Color::TRANSPARENT)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(if accent_outline {
                            theme.rgba(ThemeRole::Accent).with_alpha(0.7).to_color()
                        } else {
                            theme.color(ThemeRole::Border)
                        }),
                )
                .center()
                .cursor(CursorIcon::Pointer)
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    core_a.invoke_or_log(Command::AgentInstructionAdd);
                })
                .child(
                    label()
                        .text(label_text)
                        .font_size(12.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(theme.color(ThemeRole::Text)),
                )
        };

        let body: Element = if self.instructions.is_empty() {
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .padding(Gaps::new(8.0, 14.0, 16.0, 14.0))
                .spacing(16.0)
                .child(
                    label()
                        .text("Guidelines your agent follows on every request, such as how to answer or what to avoid. Switch one off to keep it without using it.")
                        .font_size(12.0)
                        .line_height(1.5)
                        .color(theme.color(ThemeRole::TextMuted)),
                )
                .child(add_btn("Add instruction", true))
                .into_element()
        } else {
            let mut list = rect()
                .width(Size::fill())
                .padding(Gaps::new(6.0, 12.0, 14.0, 12.0))
                .spacing(10.0);
            for item in &self.instructions {
                list = list.child(InstructionCard::new(item.clone()).key(item.id.0));
            }
            list = list.child(add_btn("+  Add instruction", false));
            ScrollView::new()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .child(list)
                .into_element()
        };

        let footer = rect()
            .width(Size::fill())
            .padding(Gaps::new(8.0, 12.0, 12.0, 12.0))
            .child(
                rect()
                    .a11y_role(AccessibilityRole::Button)
                    .a11y_alt("Done editing instructions")
                    .width(Size::fill())
                    .height(Size::px(34.0))
                    .corner_radius(CornerRadius::new_all(999.0))
                    .background(accent_col)
                    .center()
                    .cursor(CursorIcon::Pointer)
                    .on_press(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        if let Some(mut ds) = dock_state {
                            ds.instructions_open.set(false);
                        }
                    })
                    .child(
                        label()
                            .text("Done")
                            .font_size(12.5)
                            .font_weight(FontWeight::BOLD)
                            .color(theme.color(ThemeRole::TextOnAccent)),
                    ),
            );

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .content(Content::Flex)
            .background(panel_bg)
            .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if e.key == Key::Named(NamedKey::Escape)
                    && let Some(mut ds) = dock_state
                    && *ds.instructions_open.peek()
                {
                    e.stop_propagation();
                    ds.instructions_open.set(false);
                }
            })
            .child(header)
            .child(body)
            .maybe_child(
                (!self.instructions.is_empty())
                    .then(|| rect().width(Size::fill()).height(Size::px(1.0)).background(border_col)),
            )
            .child(footer)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct InstructionCard {
    item: AgentInstruction,
    key: DiffKey,
}

impl KeyExt for InstructionCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl InstructionCard {
    fn new(item: AgentInstruction) -> Self {
        Self {
            item,
            key: DiffKey::None,
        }
    }
}

impl Component for InstructionCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut title_state = use_state(|| self.item.title.clone());
        let mut text_state = use_state(|| self.item.text.clone());
        let mut last_item_title = use_state(|| self.item.title.clone());
        let mut last_item_text = use_state(|| self.item.text.clone());
        let text_a11y_id = use_a11y();

        if *last_item_title.peek() != self.item.title {
            last_item_title.set(self.item.title.clone());
            title_state.set(self.item.title.clone());
        }
        if *last_item_text.peek() != self.item.text {
            last_item_text.set(self.item.text.clone());
            text_state.set(self.item.text.clone());
        }

        let card_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(16, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(235, 248, 249, 255),
        };
        let slot_bg = match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(14, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(10, 20, 24, 38),
        };
        let enabled = self.item.enabled;
        let card_border = if enabled {
            theme.rgba(ThemeRole::Accent).with_alpha(0.55).to_color()
        } else {
            theme.color(ThemeRole::BorderSubtle)
        };

        let id = self.item.id;
        let core_toggle = core.clone();
        let core_delete = core.clone();
        let core_title = core.clone();
        let core_title_blur = core.clone();
        let core_text = core.clone();
        let core_text_blur = core.clone();
        let core_attach = core.clone();
        let core_detach = core.clone();
        let title_colors = agent_input_colors(&theme, true);
        let body_colors = agent_input_colors(&theme, false);
        let pref_color = |pref: Option<Preference<Color>>, fallback: Color| match pref {
            Some(Preference::Specific(c)) => c,
            _ => fallback,
        };

        let file_slot: Element = if let Some(file) = &self.item.file {
            let size_label = format_file_size(file.size_bytes);
            let icon_name = if file.kind == AttachmentKind::Image {
                "new-tab-image"
            } else {
                "page-portrait"
            };
            rect()
                .width(Size::px(72.0))
                .height(Size::px(72.0))
                .padding(Gaps::new(6.0, 6.0, 6.0, 6.0))
                .corner_radius(CornerRadius::new_all(12.0))
                .background(slot_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.color(ThemeRole::BorderSubtle)),
                )
                .center()
                .spacing(3.0)
                .child(Icon::new(icon_name).size(18.0).role(ThemeRole::Accent))
                .child(
                    label()
                        .text(file.name.clone())
                        .font_size(9.5)
                        .font_weight(FontWeight::MEDIUM)
                        .color(theme.color(ThemeRole::Text))
                        .max_lines(1),
                )
                .child(
                    label()
                        .text(size_label)
                        .font_size(8.5)
                        .color(theme.color(ThemeRole::TextMuted))
                        .max_lines(1),
                )
                .child(
                    rect()
                        .a11y_role(AccessibilityRole::Button)
                        .a11y_alt("Remove attached file")
                        .position(Position::new_absolute().top(3.0).right(3.0))
                        .width(Size::px(16.0))
                        .height(Size::px(16.0))
                        .corner_radius(CornerRadius::new_all(999.0))
                        .background(theme.color(ThemeRole::SurfaceRaised))
                        .center()
                        .cursor(CursorIcon::Pointer)
                        .on_press(move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            core_detach.invoke_or_log(Command::AgentInstructionSetFile { id, file: None });
                        })
                        .child(Icon::new("close").size(8.5).role(ThemeRole::TextMuted)),
                )
                .into_element()
        } else {
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Attach reference file")
                .width(Size::px(72.0))
                .height(Size::px(72.0))
                .corner_radius(CornerRadius::new_all(12.0))
                .background(slot_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.rgba(ThemeRole::Border).with_alpha(0.75).to_color()),
                )
                .center()
                .cursor(CursorIcon::Pointer)
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    core_attach.invoke_or_log(Command::AgentInstructionSetFile {
                        id,
                        file: Some(InstructionAttachment {
                            name: "style-guide.md".to_string(),
                            path: "/home/user/docs/style-guide.md".to_string(),
                            size_bytes: 4_820,
                            kind: AttachmentKind::Document,
                        }),
                    });
                })
                .child(Icon::new("plus").size(15.0).role(ThemeRole::TextMuted))
                .into_element()
        };

        rect()
            .width(Size::fill())
            .padding(Gaps::new(10.0, 12.0, 12.0, 12.0))
            .spacing(10.0)
            .corner_radius(CornerRadius::new_all(14.0))
            .background(card_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(card_border),
            )
            .opacity(if enabled { 1.0 } else { 0.65 })
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .child(KitSwitch::new(enabled).on_toggle(move |next| {
                        core_toggle.invoke_or_log(Command::AgentInstructionUpdate {
                            id,
                            enabled: Some(next),
                            title: None,
                            text: None,
                        });
                    }))
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .font_size(12.5)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .child(
                                TextField::new(title_state)
                                    .placeholder("Instruction title")
                                    .width(Size::fill())
                                    .color(pref_color(title_colors.color, theme.color(ThemeRole::Text)))
                                    .placeholder_color(pref_color(
                                        title_colors.placeholder_color,
                                        theme.color(ThemeRole::TextMuted),
                                    ))
                                    .padding(Gaps::new(2.0, 2.0, 2.0, 2.0))
                                    .corner_radius(CornerRadius::new_all(4.0))
                                    .on_change(move |val: String| {
                                        last_item_title.set(val.clone());
                                        core_title.invoke_or_log(Command::AgentInstructionUpdate {
                                            id,
                                            enabled: None,
                                            title: Some(val),
                                            text: None,
                                        });
                                    })
                                    .on_submit(move |_| {
                                        text_a11y_id.request_focus();
                                    })
                                    .on_blur(move |val: String| {
                                        if *last_item_title.peek() != val {
                                            last_item_title.set(val.clone());
                                            core_title_blur.invoke_or_log(Command::AgentInstructionUpdate {
                                                id,
                                                enabled: None,
                                                title: Some(val),
                                                text: None,
                                            });
                                        }
                                    }),
                            ),
                    )
                    .child(
                        rect()
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Delete instruction")
                            .width(Size::px(24.0))
                            .height(Size::px(24.0))
                            .corner_radius(CornerRadius::new_all(6.0))
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_delete.invoke_or_log(Command::AgentInstructionRemove { id });
                            })
                            .child(Icon::new("delete").size(13.0).role(ThemeRole::TextMuted)),
                    ),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::start())
                    .spacing(10.0)
                    .child(file_slot)
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .height(Size::px(72.0))
                            .font_size(12.0)
                            .child(
                                TextField::new(text_state)
                                    .a11y_id(text_a11y_id)
                                    .multiline(true)
                                    .placeholder("Write guidelines for the agent…")
                                    .width(Size::fill())
                                    .height(Size::px(72.0))
                                    .background(pref_color(body_colors.background, Color::TRANSPARENT))
                                    .focus_background(pref_color(body_colors.focus_background, Color::TRANSPARENT))
                                    .border(pref_color(body_colors.border_fill, Color::TRANSPARENT), 1.0)
                                    .focus_border(
                                        pref_color(body_colors.focus_border_fill, theme.color(ThemeRole::Accent)),
                                        1.25,
                                    )
                                    .color(pref_color(body_colors.color, theme.color(ThemeRole::Text)))
                                    .placeholder_color(pref_color(
                                        body_colors.placeholder_color,
                                        theme.color(ThemeRole::TextMuted),
                                    ))
                                    .padding(Gaps::new(6.0, 8.0, 6.0, 8.0))
                                    .corner_radius(CornerRadius::new_all(10.0))
                                    .on_change(move |val: String| {
                                        last_item_text.set(val.clone());
                                        core_text.invoke_or_log(Command::AgentInstructionUpdate {
                                            id,
                                            enabled: None,
                                            title: None,
                                            text: Some(val),
                                        });
                                    })
                                    .on_blur(move |val: String| {
                                        if *last_item_text.peek() != val {
                                            last_item_text.set(val.clone());
                                            core_text_blur.invoke_or_log(Command::AgentInstructionUpdate {
                                                id,
                                                enabled: None,
                                                title: None,
                                                text: Some(val),
                                            });
                                        }
                                    }),
                            ),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
