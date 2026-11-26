/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Full-panel slide-in overlay for managing reusable agent instructions (`AgentInstructionsPage`).
//!
//! Matches Zendia's `ZendiaAgentInstructionsView` (`zendia.css:2230-2553`):
//! - Animated slide overlay (`300 ms` ease-out in / `170 ms` ease-in out from `x: +28 px`).
//! - Header: `arrow-left` back `IconButton`, `15 px` SemiBold `"Agent instructions"` title, `×` close `IconButton`.
//! - Staggered card rise (`260 ms`, delays `0 / 50 / 100 / 140 ms`), dimming ONLY title & text (`0.55`) when disabled.
//! - `72×72` `radius: 16 px` dashed `+` file slot turning accent on hover, or `EXT` document tile / media preview tile
//!   with corner `×` remove button, plus inline warning error banner and auto-growing guideline textarea up to `260 px`.

use crate::ui::agent::{agent_input_colors, colored_zen_icon, dashed_rounded_box_fill};
use crate::ui::core_view::use_core;
use crate::ui::kit::{
    Icon, IconButton, KitSwitch, KitTooltip, TextField, focus_ring_border, mix_color, use_interactive_feedback,
};
use crate::ui::panels::try_use_dock_state;
use crate::ui::theme::{ResolvedScheme, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::agent::{
    AgentInstruction, AttachmentKind, InstructionAttachment, InstructionId, format_file_size,
};
use zendia_browser_core::commands::Command;

const MAX_INSTRUCTION_TITLE_CHARS: usize = 80;
const MAX_INSTRUCTION_BODY_CHARS: usize = 4000;
const MAX_INSTRUCTION_FILE_BYTES: u64 = 25 * 1024 * 1024;

/// Estimates an auto-growing textarea height for `text` inside `width` logical pixels, clamped to `[min_h, max_h]`.
pub(crate) fn estimate_autogrow_height(
    text: &str,
    width: f32,
    char_width: f32,
    line_height: f32,
    pad_y: f32,
    min_h: f32,
    max_h: f32,
) -> f32 {
    if text.is_empty() {
        return min_h;
    }
    let usable_w = (width - 20.0).max(80.0);
    let cols = (usable_w / char_width).floor().max(8.0) as usize;
    let mut lines = 0usize;
    for raw_line in text.split('\n') {
        let len = raw_line.chars().count();
        lines += if len == 0 { 1 } else { len.div_ceil(cols) };
    }
    ((lines.max(1) as f32) * line_height + pad_y).clamp(min_h, max_h)
}

/// Full-panel animated overlay editor for persistent agent instructions.
#[derive(Clone, PartialEq)]
pub struct AgentInstructionsPage {
    pub open: bool,
    pub instructions: Vec<AgentInstruction>,
    pub accent: Color,
    pub composer_a11y_id: Option<AccessibilityId>,
    pub static_snapshot: bool,
    key: DiffKey,
}

impl KeyExt for AgentInstructionsPage {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl AgentInstructionsPage {
    pub fn new(open: bool, instructions: Vec<AgentInstruction>, accent: Color) -> Self {
        Self {
            open,
            instructions,
            accent,
            composer_a11y_id: None,
            static_snapshot: false,
            key: DiffKey::None,
        }
    }

    pub fn composer_a11y_id(mut self, id: AccessibilityId) -> Self {
        self.composer_a11y_id = Some(id);
        self
    }

    pub fn static_snapshot(mut self, static_snapshot: bool) -> Self {
        self.static_snapshot = static_snapshot;
        self
    }
}

impl Component for AgentInstructionsPage {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let dock_state = try_use_dock_state();
        let mut mounted = use_state(|| self.open);
        let mut unmount_seq = use_state(|| 0u64);
        let mut last_open = use_state(|| self.open);
        let mut focus_new_id = use_state(|| None::<InstructionId>);
        let mut pending_focus_new = use_state(|| false);
        let mut prev_count = use_state(|| self.instructions.len());
        let composer_id = self.composer_a11y_id;

        if *last_open.peek() != self.open {
            last_open.set(self.open);
            if self.open {
                let next_seq = unmount_seq.peek().wrapping_add(1);
                unmount_seq.set(next_seq);
                mounted.set(true);
            } else if self.static_snapshot {
                mounted.set(false);
            } else {
                let seq = unmount_seq.peek().wrapping_add(1);
                unmount_seq.set(seq);
                if let Some(cid) = composer_id {
                    cid.request_focus();
                }
                spawn(async move {
                    timer(motion::INSTRUCTIONS_OUT).await;
                    if *unmount_seq.peek() == seq {
                        mounted.set(false);
                    }
                });
            }
        }

        // Auto-focus a newly added instruction's title field only after clicking "Add instruction".
        if *pending_focus_new.peek()
            && self.instructions.len() > *prev_count.peek()
            && let Some(last) = self.instructions.last()
        {
            pending_focus_new.set(false);
            focus_new_id.set(Some(last.id));
        }
        prev_count.set_if_modified(self.instructions.len());

        let open_anim = use_animation_with_dependencies(&(self.open, self.static_snapshot), |conf, (open, snap)| {
            if *snap {
                conf.on_creation(OnCreation::Finish);
            } else {
                conf.on_creation(if *open { OnCreation::Run } else { OnCreation::Finish });
                conf.on_change(OnChange::Rerun);
            }
            if *open {
                AnimNum::new(0.0, 1.0)
                    .duration(motion::INSTRUCTIONS_IN)
                    .ease(Ease::Out)
                    .function(motion::ease_out_fn())
            } else {
                AnimNum::new(1.0, 0.0)
                    .duration(motion::INSTRUCTIONS_OUT)
                    .ease(Ease::In)
                    .function(motion::ease_in_fn())
            }
        });

        if !*mounted.read() {
            return rect().into_element();
        }

        let progress = if self.static_snapshot {
            if self.open { 1.0 } else { 0.0 }
        } else {
            open_anim.get().value().clamp(0.0, 1.0)
        };
        let slide_x = (1.0 - progress) * 28.0;

        let panel_bg = theme.color(ThemeRole::Surface);
        let core_add = core.clone();
        let focus_target = *focus_new_id.read();

        let close_action = move || {
            if let Some(mut ds) = dock_state {
                ds.instructions_open.set(false);
            }
            if let Some(cid) = composer_id {
                cid.request_focus();
            }
        };

        let header = rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .child(
                IconButton::new("arrow-left", "Back")
                    .tooltip("Back")
                    .size(24.0)
                    .icon_size(14.0)
                    .on_press({
                        let close = close_action;
                        move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            close();
                        }
                    }),
            )
            .child(
                rect().width(Size::flex(1.0)).child(
                    label()
                        .text("Agent instructions")
                        .font_size(15.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(theme.color(ThemeRole::Text))
                        .max_lines(1),
                ),
            )
            .child(
                IconButton::new("close", "Close")
                    .tooltip("Close")
                    .shortcut("Esc")
                    .size(24.0)
                    .icon_size(14.0)
                    .on_press({
                        let close = close_action;
                        move |e: Event<PressEventData>| {
                            e.stop_propagation();
                            close();
                        }
                    }),
            );

        let add_button = InstructionAddButton {
            on_press: EventHandler::new(move |e: Event<PressEventData>| {
                e.stop_propagation();
                pending_focus_new.set(true);
                core_add.invoke_or_log(Command::AgentInstructionAdd);
            }),
            key: DiffKey::None,
        };

        let mut list = rect().width(Size::fill()).spacing(10.0);
        if self.instructions.is_empty() {
            list = list.child(
                rect().width(Size::fill()).padding(Gaps::new(4.0, 6.0, 4.0, 6.0)).child(
                    label()
                        .text("Guidelines your agent follows on every request, such as how to answer or what to avoid. Switch one off to keep it without using it.")
                        .font_size(13.0)
                        .line_height(1.45)
                        .color(theme.color(ThemeRole::TextMuted)),
                ),
            );
        } else {
            for (idx, item) in self.instructions.iter().enumerate() {
                list = list.child(
                    InstructionCard::new(item.clone(), idx, focus_target == Some(item.id), self.static_snapshot)
                        .key(item.id.0),
                );
            }
        }
        list = list.child(add_button);

        let body = ScrollView::new()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .child(list);

        let footer = InstructionDoneButton {
            on_press: EventHandler::new(move |e: Event<PressEventData>| {
                e.stop_propagation();
                close_action();
            }),
            key: DiffKey::None,
        };

        let is_open = self.open;
        rect()
            .layer(Layer::Relative(80))
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .offset_x(slide_x)
            .opacity(progress)
            .content(Content::Flex)
            .padding(Gaps::new_all(10.0))
            .spacing(12.0)
            .background(panel_bg)
            .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if is_open && e.key == Key::Named(NamedKey::Escape) {
                    e.stop_propagation();
                    e.prevent_default();
                    close_action();
                }
            })
            .child(header)
            .child(body)
            .child(footer)
            .into_element()
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct InstructionAddButton {
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for InstructionAddButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for InstructionAddButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let border_col = mix_color(
            theme.color(ThemeRole::Border),
            theme.color(ThemeRole::Accent),
            fb.hover_progress(),
        );
        let base_border = Border::new()
            .width(if fb.is_hovered() { 1.0 } else { 0.5 })
            .alignment(BorderAlignment::Inner)
            .fill(border_col);
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, base_border);
        let on_press = self.on_press.clone();

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Add instruction")
            .width(Size::fill())
            .padding(Gaps::new(9.0, 16.0, 9.0, 16.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .background(Color::TRANSPARENT)
            .border(border)
            .scale(fb.scale())
            .direction(Direction::Horizontal)
            .main_align(Alignment::center())
            .cross_align(Alignment::center())
            .spacing(8.0)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| on_press.call(e))
            .child(Icon::new("plus").size(14.0).role(ThemeRole::Text))
            .child(
                label()
                    .text("Add instruction")
                    .font_size(13.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(theme.color(ThemeRole::Text)),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct InstructionDoneButton {
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for InstructionDoneButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for InstructionDoneButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let accent = theme.color(ThemeRole::Accent);
        let hover_bg = mix_color(accent, theme.color(ThemeRole::Text), 0.15);
        let press_bg = mix_color(accent, Color::BLACK, 0.14);
        let bg = fb.background(accent, hover_bg, press_bg);
        let fg = match theme.scheme {
            ResolvedScheme::Light => Color::WHITE,
            ResolvedScheme::Dark => theme.color(ThemeRole::Ink),
        };
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, Border::new());
        let on_press = self.on_press.clone();

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Done editing instructions")
            .width(Size::fill())
            .padding(Gaps::new(9.0, 16.0, 9.0, 16.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .background(bg)
            .border(border)
            .scale(fb.scale())
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(fb.on_pointer_enter())
            .on_pointer_leave(fb.on_pointer_leave())
            .on_pointer_down(fb.on_pointer_down())
            .on_global_pointer_up(fb.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| on_press.call(e))
            .child(
                label()
                    .text("Done")
                    .font_size(13.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(fg),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct InstructionFileAddSlot {
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for InstructionFileAddSlot {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for InstructionFileAddSlot {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let fb = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let t = fb.hover_progress();
        let bg = mix_color(
            Color::TRANSPARENT,
            theme.rgba(ThemeRole::Accent).with_alpha(0.12).to_color(),
            t,
        );
        let stroke = mix_color(
            theme.rgba(ThemeRole::TextMuted).with_alpha(0.70).to_color(),
            theme.color(ThemeRole::Accent),
            t,
        );
        let icon_role = if fb.is_hovered() {
            ThemeRole::Accent
        } else {
            ThemeRole::TextMuted
        };
        let border = focus_ring_border(matches!(focus(), Focus::Keyboard), &theme, Border::new());
        let on_press = self.on_press.clone();

        KitTooltip::new("Attach a file, a picture, video or document (one per instruction)").child(
            rect()
                .a11y_id(a11y_id)
                .a11y_focusable(true)
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt("Attach a file, a picture, video or document (one per instruction)")
                .width(Size::px(72.0))
                .height(Size::px(72.0))
                .corner_radius(CornerRadius::new_all(16.0))
                .background(dashed_rounded_box_fill(bg, stroke, 1.5, 16.0, 4.0, 3.5))
                .border(border)
                .scale(fb.scale())
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(fb.on_pointer_enter())
                .on_pointer_leave(fb.on_pointer_leave())
                .on_pointer_down(fb.on_pointer_down())
                .on_global_pointer_up(fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| on_press.call(e))
                .child(Icon::new("plus").size(18.0).role(icon_role)),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct TileRemoveButton {
    file_name: String,
    on_press: EventHandler<Event<PressEventData>>,
    key: DiffKey,
}

impl KeyExt for TileRemoveButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for TileRemoveButton {
    fn render(&self) -> impl IntoElement {
        let fb = use_interactive_feedback(true);
        let bg = fb.background(
            Color::from_argb(153, 0, 0, 0),
            Color::from_argb(217, 0, 0, 0),
            Color::from_argb(235, 0, 0, 0),
        );
        let on_press = self.on_press.clone();
        let tip = format!("Remove {}", self.file_name);

        KitTooltip::new(tip.clone()).child(
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_alt(tip)
                .width(Size::px(20.0))
                .height(Size::px(20.0))
                .corner_radius(CornerRadius::new_all(999.0))
                .background(bg)
                .center()
                .cursor(CursorIcon::Pointer)
                .on_pointer_enter(fb.on_pointer_enter())
                .on_pointer_leave(fb.on_pointer_leave())
                .on_pointer_down(fb.on_pointer_down())
                .on_global_pointer_up(fb.on_pointer_up())
                .on_press(move |e: Event<PressEventData>| on_press.call(e))
                .child(colored_zen_icon("close", 10.0, Color::WHITE)),
        )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct InstructionCard {
    item: AgentInstruction,
    index: usize,
    auto_focus_title: bool,
    static_snapshot: bool,
    key: DiffKey,
}

impl KeyExt for InstructionCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl InstructionCard {
    fn new(item: AgentInstruction, index: usize, auto_focus_title: bool, static_snapshot: bool) -> Self {
        Self {
            item,
            index,
            auto_focus_title,
            static_snapshot,
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
        let mut error_msg = use_state(|| None::<String>);
        let mut textarea_width = use_state(|| 240.0f32);
        let title_a11y_id = use_a11y();
        let text_a11y_id = use_a11y();
        let title_focus = use_focus(title_a11y_id);
        let text_focus = use_focus(text_a11y_id);

        let stagger_ms = match self.index {
            0 => 0,
            1 => 50,
            2 => 100,
            _ => 140,
        };
        let rise = use_animation_with_dependencies(&self.static_snapshot, move |conf, snap| {
            if *snap {
                conf.on_creation(OnCreation::Finish);
            } else {
                conf.on_creation(OnCreation::Run);
            }
            let total = Duration::from_millis(260 + stagger_ms);
            AnimNum::new(0.0, 1.0).duration(total).function(Function::Linear)
        });

        let rise_t = if self.static_snapshot {
            1.0
        } else {
            let total = (260 + stagger_ms) as f32;
            let elapsed = rise.get().value() * total;
            let local = ((elapsed - stagger_ms as f32) / 260.0).clamp(0.0, 1.0);
            motion::zendia_ease_out(local, Ease::Out)
        };

        if *last_item_title.peek() != self.item.title {
            last_item_title.set(self.item.title.clone());
            title_state.set(self.item.title.clone());
        }
        if *last_item_text.peek() != self.item.text {
            last_item_text.set(self.item.text.clone());
            text_state.set(self.item.text.clone());
        }

        let enabled = self.item.enabled;
        let card_focused = title_focus().is_focused() || text_focus().is_focused();
        let card_bg = theme.rgba(ThemeRole::SurfaceRaised).with_alpha(0.70).to_color();
        let (card_border_w, card_border_col) = if card_focused {
            (1.0, theme.rgba(ThemeRole::Accent).with_alpha(0.70).to_color())
        } else {
            (0.5, theme.rgba(ThemeRole::Border).with_alpha(0.80).to_color())
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
        let pref_color = |pref: Option<Preference<Color>>, fallback: Color| match pref {
            Some(Preference::Specific(c)) => c,
            _ => fallback,
        };

        let file_slot: Element = if let Some(file) = &self.item.file {
            let is_media = matches!(file.kind, AttachmentKind::Image | AttachmentKind::Video);
            let tooltip_text = format!("{} \u{00b7} {}", file.name, format_file_size(file.size_bytes));
            let ext = file
                .name
                .rsplit_once('.')
                .map(|(_, e)| e.chars().take(4).collect::<String>().to_ascii_uppercase())
                .filter(|e| !e.is_empty())
                .unwrap_or_else(|| "FILE".to_string());

            let tile_bg = if is_media {
                theme.rgba(ThemeRole::Text).with_alpha(0.10).to_color()
            } else {
                theme.rgba(ThemeRole::Accent).with_alpha(0.16).to_color()
            };

            let remove_btn = rect()
                .layer(Layer::Relative(2))
                .position(Position::new_absolute().top(4.0).right(4.0))
                .child(TileRemoveButton {
                    file_name: file.name.clone(),
                    on_press: EventHandler::new(move |e: Event<PressEventData>| {
                        e.stop_propagation();
                        error_msg.set_if_modified(None);
                        core_detach.invoke_or_log(Command::AgentInstructionSetFile { id, file: None });
                    }),
                    key: DiffKey::None,
                });

            let inner: Element = if is_media {
                let is_video = file.kind == AttachmentKind::Video;
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .center()
                    .spacing(3.0)
                    .child(Icon::new("new-tab-image").size(22.0).role(ThemeRole::TextMuted))
                    .child(
                        label()
                            .text(file.name.clone())
                            .font_size(9.5)
                            .color(theme.color(ThemeRole::TextMuted))
                            .max_lines(1),
                    )
                    .maybe_child(is_video.then(|| {
                        rect()
                            .position(Position::new_absolute().top(23.0).left(23.0))
                            .width(Size::px(26.0))
                            .height(Size::px(26.0))
                            .corner_radius(CornerRadius::new_all(999.0))
                            .background(Color::from_argb(140, 0, 0, 0))
                            .center()
                            .child(colored_zen_icon("media-play", 12.0, Color::WHITE))
                    }))
                    .into_element()
            } else {
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .padding(Gaps::new_all(6.0))
                    .center()
                    .spacing(4.0)
                    .child(
                        label()
                            .text(ext)
                            .font_size(15.0)
                            .font_weight(FontWeight::BOLD)
                            .color(theme.color(ThemeRole::Accent))
                            .max_lines(1),
                    )
                    .child(
                        label()
                            .text(file.name.clone())
                            .font_size(10.0)
                            .color(theme.color(ThemeRole::TextMuted))
                            .max_lines(1),
                    )
                    .into_element()
            };

            KitTooltip::new(tooltip_text)
                .child(
                    rect()
                        .width(Size::px(72.0))
                        .height(Size::px(72.0))
                        .corner_radius(CornerRadius::new_all(16.0))
                        .overflow(Overflow::Clip)
                        .background(tile_bg)
                        .border(
                            Border::new()
                                .width(0.5)
                                .alignment(BorderAlignment::Inner)
                                .fill(theme.rgba(ThemeRole::Border).with_alpha(0.80).to_color()),
                        )
                        .child(inner)
                        .child(remove_btn),
                )
                .into_element()
        } else {
            InstructionFileAddSlot {
                on_press: EventHandler::new(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    let sample = InstructionAttachment {
                        name: "style-guide.md".to_string(),
                        path: "/home/user/docs/style-guide.md".to_string(),
                        size_bytes: 4_820,
                        kind: AttachmentKind::Document,
                    };
                    if sample.size_bytes > MAX_INSTRUCTION_FILE_BYTES {
                        error_msg.set(Some(format!("{} is over 25 MB", sample.name)));
                        return;
                    }
                    error_msg.set_if_modified(None);
                    core_attach.invoke_or_log(Command::AgentInstructionSetFile { id, file: Some(sample) });
                }),
                key: DiffKey::None,
            }
            .into_element()
        };

        let text_height =
            estimate_autogrow_height(&text_state.read(), *textarea_width.read(), 7.2, 19.0, 16.0, 72.0, 260.0);
        let text_bg = theme.rgba(ThemeRole::Surface).with_alpha(0.60).to_color();
        let dim_opacity = if enabled { 1.0 } else { 0.55 };
        let err = error_msg.read().clone();

        rect()
            .width(Size::fill())
            .offset_y((1.0 - rise_t) * 10.0)
            .opacity(rise_t)
            .padding(Gaps::new_all(12.0))
            .spacing(8.0)
            .corner_radius(CornerRadius::new_all(16.0))
            .background(card_bg)
            .border(
                Border::new()
                    .width(card_border_w)
                    .alignment(BorderAlignment::Inner)
                    .fill(card_border_col),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::center())
                    .spacing(10.0)
                    .child(
                        KitTooltip::new("Follow this instruction").child(KitSwitch::new(enabled).on_toggle(
                            move |next| {
                                core_toggle.invoke_or_log(Command::AgentInstructionUpdate {
                                    id,
                                    enabled: Some(next),
                                    title: None,
                                    text: None,
                                });
                            },
                        )),
                    )
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .opacity(dim_opacity)
                            .font_size(14.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .child(
                                TextField::new(title_state)
                                    .a11y_id(title_a11y_id)
                                    .auto_focus(self.auto_focus_title)
                                    .max_chars(MAX_INSTRUCTION_TITLE_CHARS)
                                    .placeholder("Instruction title")
                                    .width(Size::fill())
                                    .color(pref_color(title_colors.color, theme.color(ThemeRole::Text)))
                                    .placeholder_color(pref_color(
                                        title_colors.placeholder_color,
                                        theme.color(ThemeRole::TextMuted),
                                    ))
                                    .padding(Gaps::new(2.0, 0.0, 2.0, 0.0))
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
                        IconButton::new("trash", "Delete this instruction")
                            .tooltip("Delete this instruction")
                            .size(24.0)
                            .icon_size(14.0)
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                core_delete.invoke_or_log(Command::AgentInstructionRemove { id });
                            }),
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
                            .height(Size::px(text_height))
                            .opacity(dim_opacity)
                            .font_size(13.0)
                            .on_sized(move |e: Event<SizedEventData>| {
                                let w = e.area.width();
                                if w > 20.0 && (*textarea_width.peek() - w).abs() > 1.0 {
                                    textarea_width.set(w);
                                }
                            })
                            .child(
                                TextField::new(text_state)
                                    .a11y_id(text_a11y_id)
                                    .multiline(true)
                                    .max_chars(MAX_INSTRUCTION_BODY_CHARS)
                                    .placeholder("Create a guideline for your agent")
                                    .width(Size::fill())
                                    .height(Size::px(text_height))
                                    .background(text_bg)
                                    .focus_background(text_bg)
                                    .color(theme.color(ThemeRole::Text))
                                    .placeholder_color(theme.color(ThemeRole::TextMuted))
                                    .padding(Gaps::new(8.0, 10.0, 8.0, 10.0))
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
            .maybe_child(err.map(|msg| label().text(msg).font_size(11.0).color(theme.color(ThemeRole::Warning))))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
