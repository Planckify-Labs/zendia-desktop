/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Shared editable text field built on `freya::text_edit::use_editable`.
//!
//! Adds the behaviors Freya 0.5.4's built-in `Input` omits:
//! - Programmatic select-all on open (or on sequence bump), with proper `ArrowLeft`/`ArrowRight` collapse to the
//!   start/end of the selection and pointer-click editing.
//! - Focus loss and outside-click (`on_blur`) callbacks so inline editors commit or cancel like the fork.
//! - Live `on_change` notifications on every mutation (typing, paste, cut, undo/redo, editing chords).
//! - Built-in `Ctrl+W` (delete previous word), `Ctrl+U` (delete to line start), and `Ctrl+H` (delete previous
//!   character) editing keys, integrated with undo history and guarded from leaking to browser shortcuts via
//!   [`TextFocusTracker`].

use crate::ui::theme::{ThemeRole, use_theme};
use freya::animation::*;
use freya::elements::paragraph::ParagraphCursorExt;
use freya::prelude::*;
use freya::text_edit::*;
use freya::torin::prelude::{Point2D, Vector2D};
use std::borrow::Cow;
use std::ops::Range;
use std::time::Duration;

/// Window-level tracker recording which [`TextField`] currently owns keyboard focus so global shortcut handlers never
/// steal text-editing keys (`Ctrl+W`, `Ctrl+U`, `Ctrl+H`, `Ctrl+A/C/X/V/Z/Y`, printable keys).
#[derive(Clone, Copy, PartialEq)]
pub struct TextFocusTracker {
    focused_field: State<Option<AccessibilityId>>,
}

impl TextFocusTracker {
    /// Marks `id` as the currently focused text field.
    pub fn mark_focused(&self, id: AccessibilityId) {
        let mut state = self.focused_field;
        state.set_if_modified(Some(id));
    }

    /// Clears `id` if it is currently recorded as focused.
    pub fn mark_unfocused(&self, id: AccessibilityId) {
        let mut state = self.focused_field;
        if *state.peek() == Some(id) {
            state.set(None);
        }
    }

    /// Returns `true` while any [`TextField`] in this window holds keyboard focus.
    pub fn is_any_focused(&self) -> bool {
        self.focused_field.peek().is_some_and(|id| id.is_focused())
    }
}

/// Installs [`TextFocusTracker`] in Freya context for the current window.
pub fn use_provide_text_focus() -> TextFocusTracker {
    let focused_field = use_state(|| None::<AccessibilityId>);
    use_provide_context(|| TextFocusTracker { focused_field })
}

/// Consumes [`TextFocusTracker`] if a provider is installed in this window.
pub fn try_use_text_focus() -> Option<TextFocusTracker> {
    try_consume_context::<TextFocusTracker>()
}

/// Returns `true` if `e` is a text-editing or typing key that must stay inside a focused text input and never fall
/// through to browser global shortcuts (such as `Ctrl+W` closing the tab).
pub fn is_text_editing_key(e: &KeyboardEventData) -> bool {
    let ctrl_only =
        e.modifiers.contains(Modifiers::CONTROL) && !e.modifiers.intersects(Modifiers::ALT | Modifiers::META);
    let plain_or_shift = !e
        .modifiers
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META);

    if plain_or_shift {
        return !matches!(e.key, Key::Named(NamedKey::Escape));
    }

    if ctrl_only {
        match &e.key {
            Key::Named(
                NamedKey::Backspace
                | NamedKey::Delete
                | NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::Home
                | NamedKey::End,
            ) => return true,
            Key::Character(ch) => {
                let lower = ch.to_ascii_lowercase();
                if matches!(lower.as_str(), "w" | "u" | "h" | "a" | "c" | "x" | "v" | "z" | "y") {
                    return true;
                }
            }
            _ => {}
        }
        if matches!(
            e.code,
            Code::KeyW
                | Code::KeyU
                | Code::KeyH
                | Code::KeyA
                | Code::KeyC
                | Code::KeyX
                | Code::KeyV
                | Code::KeyZ
                | Code::KeyY
        ) {
            return true;
        }
    }

    false
}

/// Computes the UTF-16 deletion range for `Ctrl+W` (delete word before cursor, matching the fork's regex
/// `/(?:\w+|[^\w\s]+)?\s*$/` and joining lines when at line start).
pub fn delete_word_back_range(text: &str, cursor_utf16: usize) -> Option<Range<usize>> {
    if cursor_utf16 == 0 || text.is_empty() {
        return None;
    }
    let byte_end = utf16_to_byte_offset(text, cursor_utf16);
    let before = &text[..byte_end];
    if before.is_empty() {
        return None;
    }

    // If right after a newline, join with the previous line.
    if before.ends_with('\n') {
        let start_byte = byte_end - 1;
        return Some(byte_to_utf16_offset(text, start_byte)..cursor_utf16);
    }

    // 1. Skip trailing horizontal whitespace (spaces/tabs, stopping at newline).
    let mut start_byte = byte_end;
    for (idx, ch) in before.char_indices().rev() {
        if ch == '\n' {
            break;
        }
        if ch.is_whitespace() {
            start_byte = idx;
        } else {
            break;
        }
    }

    // 2. Skip one run of either word chars (`alphanumeric` or `_`) or non-word non-whitespace chars.
    if start_byte > 0
        && let Some((last_idx, last_ch)) = before[..start_byte].char_indices().next_back()
        && last_ch != '\n'
    {
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let want_word = is_word(last_ch);
        start_byte = last_idx;
        for (idx, ch) in before[..last_idx].char_indices().rev() {
            if ch == '\n' || ch.is_whitespace() || is_word(ch) != want_word {
                break;
            }
            start_byte = idx;
        }
    }

    if start_byte == byte_end {
        return None;
    }
    Some(byte_to_utf16_offset(text, start_byte)..cursor_utf16)
}

/// Computes the UTF-16 deletion range for `Ctrl+U` (delete back to start of current line, or join with previous line
/// when already at line start).
pub fn delete_line_back_range(text: &str, cursor_utf16: usize) -> Option<Range<usize>> {
    if cursor_utf16 == 0 || text.is_empty() {
        return None;
    }
    let byte_end = utf16_to_byte_offset(text, cursor_utf16);
    let before = &text[..byte_end];
    if before.is_empty() {
        return None;
    }
    let start_byte = match before.rfind('\n') {
        Some(nl_idx) if nl_idx + 1 == byte_end => nl_idx,
        Some(nl_idx) => nl_idx + 1,
        None => 0,
    };
    if start_byte == byte_end {
        return None;
    }
    Some(byte_to_utf16_offset(text, start_byte)..cursor_utf16)
}

fn utf16_to_byte_offset(text: &str, utf16_offset: usize) -> usize {
    let mut seen = 0usize;
    for (byte_idx, ch) in text.char_indices() {
        if seen >= utf16_offset {
            return byte_idx;
        }
        seen += ch.len_utf16();
    }
    text.len()
}

fn byte_to_utf16_offset(text: &str, byte_offset: usize) -> usize {
    text[..byte_offset.min(text.len())].encode_utf16().count()
}

/// Applies `Ctrl+W`, `Ctrl+U`, or `Ctrl+H` to `editor`. Returns `true` if handled as an editing chord.
fn apply_ctrl_delete_chord(editor: &mut RopeEditor, key: &Key, code: Code, modifiers: Modifiers) -> Option<bool> {
    if !modifiers.contains(Modifiers::CONTROL)
        || modifiers.intersects(Modifiers::ALT | Modifiers::META | Modifiers::SHIFT)
    {
        return None;
    }

    let action = match (key, code) {
        (Key::Character(ch), _) if ch.eq_ignore_ascii_case("w") => 'w',
        (_, Code::KeyW) => 'w',
        (Key::Character(ch), _) if ch.eq_ignore_ascii_case("u") => 'u',
        (_, Code::KeyU) => 'u',
        (Key::Character(ch), _) if ch.eq_ignore_ascii_case("h") => 'h',
        (_, Code::KeyH) => 'h',
        _ => return None,
    };

    if let Some((start, end)) = editor.get_selection_range() {
        editor.remove(start..end);
        editor.move_cursor_to(start);
        editor.clear_selection();
        return Some(true);
    }

    let cursor = editor.cursor_pos();
    if cursor == 0 {
        return Some(false);
    }
    let text = editor.committed_text();
    let range = match action {
        'w' => delete_word_back_range(&text, cursor),
        'u' => delete_line_back_range(&text, cursor),
        'h' => Some(editor.grapheme_cluster_at(cursor - 1).start..cursor),
        _ => None,
    };
    if let Some(range) = range {
        let new_pos = range.start;
        editor.remove(range);
        editor.move_cursor_to(new_pos);
        editor.clear_selection();
        Some(true)
    } else {
        Some(false)
    }
}

/// Returns whether a key event carries a modifier that should NOT be inserted as literal text by `RopeEditor`.
fn is_unhandled_modified_char(key: &Key, modifiers: Modifiers) -> bool {
    if !modifiers.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META) {
        return false;
    }
    let ctrl_or_meta = modifiers.contains(Modifiers::CONTROL) || modifiers.contains(Modifiers::META);
    let no_alt = !modifiers.contains(Modifiers::ALT);
    match key {
        Key::Character(ch) if ctrl_or_meta && no_alt => {
            !matches!(ch.to_ascii_lowercase().as_str(), "a" | "c" | "x" | "v" | "z" | "y")
        }
        Key::Character(_) => true,
        _ => false,
    }
}

/// Shared Zendia text field supporting select-all-on-open, blur/outside-click callbacks, live `on_change`,
/// `Ctrl+W`/`Ctrl+U`/`Ctrl+H` editing chords, and proper arrow collapse on selections.
#[derive(Clone, PartialEq)]
pub struct TextField {
    value: State<String>,
    placeholder: Option<Cow<'static, str>>,
    a11y_id: Option<AccessibilityId>,
    auto_focus: bool,
    select_all_on_open: bool,
    select_all_seq: u64,
    multiline: bool,
    enabled: bool,
    width: Size,
    height: Size,
    padding: Gaps,
    corner_radius: CornerRadius,
    background: Color,
    focus_background: Color,
    border_fill: Color,
    focus_border_fill: Color,
    border_width: f32,
    focus_border_width: f32,
    color: Option<Color>,
    placeholder_color: Option<Color>,
    text_align: TextAlign,
    max_chars: Option<usize>,
    on_change: Option<EventHandler<String>>,
    on_submit: Option<EventHandler<String>>,
    on_blur: Option<EventHandler<String>>,
    on_pre_key_down: Option<Callback<Event<KeyboardEventData>, bool>>,
    key: DiffKey,
}

impl KeyExt for TextField {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl TextField {
    pub fn new(value: State<String>) -> Self {
        Self {
            value,
            placeholder: None,
            a11y_id: None,
            auto_focus: false,
            select_all_on_open: false,
            select_all_seq: 0,
            multiline: false,
            enabled: true,
            width: Size::fill(),
            height: Size::Inner,
            padding: Gaps::new(2.0, 4.0, 2.0, 4.0),
            corner_radius: CornerRadius::new_all(0.0),
            background: Color::TRANSPARENT,
            focus_background: Color::TRANSPARENT,
            border_fill: Color::TRANSPARENT,
            focus_border_fill: Color::TRANSPARENT,
            border_width: 0.0,
            focus_border_width: 0.0,
            color: None,
            placeholder_color: None,
            text_align: TextAlign::default(),
            max_chars: None,
            on_change: None,
            on_submit: None,
            on_blur: None,
            on_pre_key_down: None,
            key: DiffKey::None,
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<Cow<'static, str>>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn a11y_id(mut self, id: AccessibilityId) -> Self {
        self.a11y_id = Some(id);
        self
    }

    pub fn auto_focus(mut self, auto_focus: bool) -> Self {
        self.auto_focus = auto_focus;
        self
    }

    /// Selects the full existing text when the field opens (or whenever [`Self::select_all_seq`] increments).
    pub fn select_all_on_open(mut self, select_all: bool) -> Self {
        self.select_all_on_open = select_all;
        self
    }

    /// Sequence counter that re-triggers programmatic select-all (and focus when `auto_focus` is set) each time it
    /// changes.
    pub fn select_all_seq(mut self, seq: u64) -> Self {
        self.select_all_seq = seq;
        self
    }

    pub fn multiline(mut self, multiline: bool) -> Self {
        self.multiline = multiline;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn width(mut self, width: Size) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: Size) -> Self {
        self.height = height;
        self
    }

    pub fn padding(mut self, padding: Gaps) -> Self {
        self.padding = padding;
        self
    }

    pub fn corner_radius(mut self, radius: CornerRadius) -> Self {
        self.corner_radius = radius;
        self
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    pub fn focus_background(mut self, color: Color) -> Self {
        self.focus_background = color;
        self
    }

    pub fn border(mut self, fill: Color, width: f32) -> Self {
        self.border_fill = fill;
        self.border_width = width;
        self
    }

    pub fn focus_border(mut self, fill: Color, width: f32) -> Self {
        self.focus_border_fill = fill;
        self.focus_border_width = width;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn placeholder_color(mut self, color: Color) -> Self {
        self.placeholder_color = Some(color);
        self
    }

    pub fn text_align(mut self, align: TextAlign) -> Self {
        self.text_align = align;
        self
    }

    pub fn max_chars(mut self, max_chars: usize) -> Self {
        self.max_chars = Some(max_chars);
        self
    }

    /// Called after every user text edit with the new string value.
    pub fn on_change(mut self, handler: impl Into<EventHandler<String>>) -> Self {
        self.on_change = Some(handler.into());
        self
    }

    /// Called when `Enter` is pressed on a single-line field.
    pub fn on_submit(mut self, handler: impl Into<EventHandler<String>>) -> Self {
        self.on_submit = Some(handler.into());
        self
    }

    /// Called when the field loses focus or the user clicks outside it.
    pub fn on_blur(mut self, handler: impl Into<EventHandler<String>>) -> Self {
        self.on_blur = Some(handler.into());
        self
    }

    /// Pre-handler called before the field processes a key event. Return `true` to continue, `false` to consume.
    pub fn on_pre_key_down(mut self, cb: impl Into<Callback<Event<KeyboardEventData>, bool>>) -> Self {
        self.on_pre_key_down = Some(cb.into());
        self
    }
}

#[derive(Clone, Copy)]
struct CursorFollower {
    a11y_id: AccessibilityId,
    display_placeholder: bool,
    holder: State<ParagraphHolder>,
    viewport_area: State<Area>,
    text_area: State<Area>,
    editable: UseEditable,
    scroll_controller: ScrollController,
    text_align: TextAlign,
    padding: Gaps,
    multiline: bool,
}

impl CursorFollower {
    fn run(mut self) {
        if !self.a11y_id.is_focused() || self.display_placeholder {
            return;
        }
        let holder_guard = self.holder.peek();
        let inner = holder_guard.0.borrow();
        let Some(holder_inner) = inner.as_ref() else {
            return;
        };
        let paragraph = &holder_inner.paragraph;
        let scale_factor = holder_inner.scale_factor;
        let viewport = *self.viewport_area.peek();
        if viewport.width() <= 0.0 {
            return;
        }
        let editor = self.editable.editor().peek();
        let text = editor.rope().to_string();
        let cursor_rect = paragraph.cursor_rect(&text, editor.cursor_pos(), self.text_align);
        let cursor_loc = Point2D::new(cursor_rect.left, cursor_rect.top) / (scale_factor as f32);
        let visible_loc = cursor_loc + Vector2D::new(self.padding.left(), self.padding.top());
        let visible_start_x = viewport.min_x() - self.text_area.peek().min_x();

        if visible_loc.x < visible_start_x {
            self.scroll_controller.scroll_to_x(-cursor_loc.x as i32);
        } else if visible_loc.x > visible_start_x + viewport.width() {
            self.scroll_controller
                .scroll_to_x(-(visible_loc.x + self.padding.right() - viewport.width()) as i32);
        }

        if self.multiline {
            let cursor_bottom = cursor_rect.bottom / (scale_factor as f32) + self.padding.bottom();
            let visible_start_y = viewport.min_y() - self.text_area.peek().min_y();
            if visible_loc.y < visible_start_y {
                self.scroll_controller.scroll_to_y(-cursor_loc.y as i32);
            } else if cursor_bottom > visible_start_y + viewport.height() {
                self.scroll_controller
                    .scroll_to_y(-(cursor_bottom - viewport.height()) as i32);
            }
        }
    }
}

impl Component for TextField {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let focus_tracker = try_use_text_focus();
        let a11y_id = use_hook(|| self.a11y_id.unwrap_or_else(AccessibilityId::new_unique));
        let focus = use_focus(a11y_id);
        let holder = use_state(ParagraphHolder::default);
        let mut outer_area = use_state(Area::default);
        let mut text_area = use_state(Area::default);
        let mut viewport_area = use_state(Area::default);
        let mut scroll_controller = use_scroll_controller(ScrollConfig::default);
        let mut is_dragging = use_state(|| false);
        let mut was_focused = use_state(|| false);
        let mut blur_fired = use_state(|| false);
        let mut applied_seq = use_state(|| None::<u64>);

        let mut editable = use_editable(
            || self.value.read().to_string(),
            || EditableConfig::new().with_allow_write_clipboard(true),
        );

        // Clear the focus tracker if this field unmounts while focused.
        use_drop(move || {
            if let Some(tracker) = focus_tracker {
                tracker.mark_unfocused(a11y_id);
            }
        });

        let mut value = self.value;
        let is_focused = focus().is_focused();

        // Keep the window-level text focus tracker in sync.
        if let Some(tracker) = focus_tracker {
            if is_focused {
                tracker.mark_focused(a11y_id);
            } else {
                tracker.mark_unfocused(a11y_id);
            }
        }

        // Sync external state writes into RopeEditor.
        let external_text = value.read().clone();
        if external_text != editable.editor().peek().committed_text() {
            let mut editor = editable.editor_mut().write();
            editor.clear_preedit();
            editor.set(&external_text);
            editor.editor_history_mut().clear();
            editor.clear_selection();
        }

        // Programmatic select-all & auto-focus on mount or when select_all_seq advances.
        let needs_open_init = *applied_seq.peek() != Some(self.select_all_seq);
        if needs_open_init {
            applied_seq.set(Some(self.select_all_seq));
            if self.auto_focus && self.enabled {
                a11y_id.request_focus();
            }
            if self.select_all_on_open {
                let mut editor = editable.editor_mut().write();
                let len = editor.len_utf16_cu();
                if len > 0 {
                    editor.set_selection((0, len));
                }
            }
        }

        // Detect focus transitions so `on_blur` fires once on focus loss.
        if is_focused {
            was_focused.set_if_modified(true);
            blur_fired.set_if_modified(false);
        } else if *was_focused.peek() {
            was_focused.set(false);
            if !*blur_fired.peek() {
                blur_fired.set(true);
                if let Some(on_blur) = &self.on_blur {
                    on_blur.call(editable.editor().peek().committed_text());
                }
            }
        }

        // Subtle caret blink that stops animating when unfocused.
        let blink = use_animation_with_dependencies(&is_focused, |conf, active| {
            if *active {
                conf.on_creation(OnCreation::Run);
                conf.on_change(OnChange::Rerun);
                conf.on_finish(OnFinish::reverse_with_delay(Duration::from_millis(520)));
            } else {
                conf.on_creation(OnCreation::Finish);
            }
            AnimNum::new(255.0, 25.0).duration(Duration::from_millis(100))
        });

        let text_color = self.color.unwrap_or_else(|| theme.color(ThemeRole::Text));
        let placeholder_color = self
            .placeholder_color
            .unwrap_or_else(|| theme.color(ThemeRole::TextMuted));
        let cursor_color = text_color.with_a(blink.get().value().clamp(0.0, 255.0) as u8);
        let selection_highlight = theme.rgba(ThemeRole::Accent).with_alpha(0.32).to_color();

        let display_placeholder =
            external_text.is_empty() && self.placeholder.is_some() && !editable.editor().read().has_preedit();

        let follower = CursorFollower {
            a11y_id,
            display_placeholder,
            holder,
            viewport_area,
            text_area,
            editable,
            scroll_controller,
            text_align: self.text_align,
            padding: self.padding,
            multiline: self.multiline,
        };
        let multiline = self.multiline;

        let on_ime_preedit = move |e: Event<ImePreeditEventData>| {
            let mut editor = editable.editor_mut().write();
            if e.data().text.is_empty() {
                editor.clear_preedit();
            } else {
                editor.set_preedit(&e.data().text);
            }
        };

        let on_pre_key_down = self.on_pre_key_down.clone();
        let on_submit = self.on_submit.clone();
        let on_change = self.on_change.clone();
        let max_chars = self.max_chars;

        let on_key_down = move |e: Event<KeyboardEventData>| {
            let key = e.key.clone();
            let code = e.code;
            let modifiers = e.modifiers;

            if let Some(cb) = &on_pre_key_down {
                if !cb.call(e) {
                    return;
                }
            } else {
                match &key {
                    Key::Named(NamedKey::Tab) => return,
                    Key::Named(NamedKey::Escape) => {}
                    _ => {
                        e.stop_propagation();
                        e.prevent_default();
                    }
                }
            }

            // 1. Enter on single-line submits.
            if matches!(&key, Key::Named(NamedKey::Enter)) && !multiline {
                if let Some(submit) = &on_submit {
                    submit.call(editable.editor().peek().committed_text());
                }
                return;
            }

            // 2. Escape unfocuses when not consumed by `on_pre_key_down`.
            if matches!(&key, Key::Named(NamedKey::Escape)) {
                a11y_id.request_unfocus();
                return;
            }

            // 3. Fix Freya's ArrowLeft/ArrowRight selection collapse (collapse to start on Left, end on Right).
            let plain_arrow =
                !modifiers.intersects(Modifiers::SHIFT | Modifiers::CONTROL | Modifiers::ALT | Modifiers::META);
            if plain_arrow && matches!(&key, Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight)) {
                let selection = editable.editor().peek().get_selection_range();
                if let Some((sel_start, sel_end)) = selection {
                    let target = if matches!(&key, Key::Named(NamedKey::ArrowLeft)) {
                        sel_start
                    } else {
                        sel_end
                    };
                    let mut editor = editable.editor_mut().write();
                    *editor.selection_mut() = TextSelection::Cursor(target);
                    drop(editor);
                    follower.run();
                    return;
                }
            }

            // 4. Fork's Ctrl+W / Ctrl+U / Ctrl+H editing chords (never leak to browser shortcuts).
            if let Some(changed) = {
                let mut editor = editable.editor_mut().write();
                apply_ctrl_delete_chord(&mut editor, &key, code, modifiers)
            } {
                if changed {
                    let next = editable.editor().peek().committed_text();
                    value.set(next.clone());
                    if let Some(handler) = &on_change {
                        handler.call(next);
                    }
                    follower.run();
                }
                return;
            }

            // 5. Never let unknown Ctrl/Alt/Meta character chords insert literal characters.
            if is_unhandled_modified_char(&key, modifiers) {
                return;
            }

            // 6. Standard editing via freya-edit.
            let prev_text = editable.editor().peek().committed_text();
            editable.process_event(EditableEvent::KeyDown {
                key: &key,
                modifiers,
                editor_line: Some(EditorLine::SingleParagraph),
                holder: Some(&holder.read()),
            });

            let mut text = editable.editor().peek().committed_text();
            if let Some(limit) = max_chars
                && text.chars().count() > limit
            {
                let mut editor = editable.editor_mut().write();
                if let Some(selection) = editor.undo() {
                    *editor.selection_mut() = selection;
                }
                editor.editor_history_mut().clear_redos();
                text = editor.committed_text();
            }

            if text != prev_text {
                value.set(text.clone());
                if let Some(handler) = &on_change {
                    handler.call(text);
                }
            }
            follower.run();
        };

        let on_key_up = move |e: Event<KeyboardEventData>| {
            e.stop_propagation();
            editable.process_event(EditableEvent::KeyUp { key: &e.key });
        };

        let on_input_focus_press = move |e: Event<FocusPressEventData>| {
            e.stop_propagation();
            e.prevent_default();
            is_dragging.set_if_modified(true);
            if !display_placeholder {
                let area = text_area.read().to_f64();
                let global_loc = e.global_location().clamp(area.min(), area.max());
                let location = (global_loc - area.min()).to_point();
                editable.process_event(EditableEvent::Down {
                    location,
                    editor_line: EditorLine::SingleParagraph,
                    holder: &holder.read(),
                });
            }
            a11y_id.request_focus();
        };

        let on_para_focus_press = move |e: Event<FocusPressEventData>| {
            e.stop_propagation();
            e.prevent_default();
            is_dragging.set_if_modified(true);
            if !display_placeholder {
                editable.process_event(EditableEvent::Down {
                    location: e.element_location(),
                    editor_line: EditorLine::SingleParagraph,
                    holder: &holder.read(),
                });
            }
            a11y_id.request_focus();
        };

        let on_global_pointer_move = move |e: Event<PointerEventData>| {
            if a11y_id.is_focused() && *is_dragging.read() {
                let area = text_area.read().to_f64();
                let location = (e.global_location() - area.min()).to_point();
                editable.process_event(EditableEvent::Move {
                    location,
                    editor_line: EditorLine::SingleParagraph,
                    holder: &holder.read(),
                });
                follower.run();
            }
        };

        let on_blur_outside = self.on_blur.clone();
        let on_global_pointer_down = move |e: Event<PointerEventData>| {
            let bounds = *outer_area.peek();
            if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
                return;
            }
            let pt = e.global_location().to_f32();
            if !bounds.contains(pt) && (a11y_id.is_focused() || *was_focused.peek()) {
                is_dragging.set_if_modified(false);
                a11y_id.request_unfocus();
                was_focused.set(false);
                if !*blur_fired.peek() {
                    blur_fired.set(true);
                    if let Some(handler) = &on_blur_outside {
                        handler.call(editable.editor().peek().committed_text());
                    }
                }
            }
        };

        let on_global_pointer_up = move |_: Event<PointerEventData>| {
            if *is_dragging.peek() {
                editable.process_event(EditableEvent::Release);
                is_dragging.set(false);
            }
        };

        let on_paragraph_sized = move |e: Event<SizedEventData>| {
            let viewport = *viewport_area.peek();
            if viewport.width() > 0.0 {
                scroll_controller.apply_layout(e.area.size, viewport.size);
            }
            let changed = text_area.peek().size != e.area.size;
            text_area.set_if_modified(e.area);
            if changed {
                follower.run();
            }
        };

        let (background, cursor_index, text_selection) = if self.enabled && is_focused {
            (
                self.focus_background,
                Some(editable.editor().read().cursor_pos()),
                editable
                    .editor()
                    .read()
                    .get_visible_selection(EditorLine::SingleParagraph),
            )
        } else {
            (self.background, None, None)
        };

        let border = if is_focused && self.focus_border_width > 0.0 {
            Border::new()
                .width(self.focus_border_width)
                .alignment(BorderAlignment::Inner)
                .fill(self.focus_border_fill)
        } else if self.border_width > 0.0 {
            Border::new()
                .width(self.border_width)
                .alignment(BorderAlignment::Inner)
                .fill(self.border_fill)
        } else {
            Border::new()
        };

        let shown_color = if display_placeholder {
            placeholder_color
        } else {
            text_color
        };

        let a11y_role = if self.multiline {
            AccessibilityRole::MultilineTextInput
        } else {
            AccessibilityRole::TextInput
        };
        let a11y_alt: String = if display_placeholder {
            self.placeholder.as_deref().unwrap_or_default().to_string()
        } else {
            external_text
        };

        rect()
            .a11y_id(a11y_id)
            .a11y_enabled(self.enabled)
            .a11y_focusable(self.enabled)
            .a11y_auto_focus(self.auto_focus)
            .a11y_alt(a11y_alt)
            .a11y_role(a11y_role)
            .width(self.width.clone())
            .height(self.height.clone())
            .background(background)
            .border(border)
            .corner_radius(self.corner_radius)
            .content(Content::Flex)
            .direction(Direction::Horizontal)
            .cross_align(if self.multiline {
                Alignment::Start
            } else {
                Alignment::center()
            })
            .cursor(if self.enabled {
                CursorIcon::Text
            } else {
                CursorIcon::NotAllowed
            })
            .on_sized(move |e: Event<SizedEventData>| outer_area.set_if_modified(e.area))
            .maybe(self.enabled, |el| {
                el.on_key_down(on_key_down)
                    .on_key_up(on_key_up)
                    .on_focus_press(on_input_focus_press)
                    .on_ime_preedit(on_ime_preedit)
                    .on_global_pointer_down(on_global_pointer_down)
                    .on_global_pointer_move(on_global_pointer_move)
                    .on_global_pointer_up(on_global_pointer_up)
            })
            .child(
                ScrollView::new_controlled(scroll_controller)
                    .width(Size::flex(1.0))
                    .height(if self.multiline && !matches!(self.height, Size::Inner) {
                        Size::fill()
                    } else {
                        Size::Inner
                    })
                    .direction(if self.multiline {
                        Direction::Vertical
                    } else {
                        Direction::Horizontal
                    })
                    .show_scrollbar(self.multiline)
                    .on_sized(move |e: Event<SizedEventData>| viewport_area.set_if_modified(e.area))
                    .child(
                        paragraph()
                            .holder(holder.read().clone())
                            .on_sized(on_paragraph_sized)
                            .min_width(Size::percent(100.0))
                            .maybe(self.multiline, |el| el.max_width(Size::percent(100.0)))
                            .maybe(self.enabled, |el| el.on_focus_press(on_para_focus_press))
                            .padding(self.padding)
                            .cursor_index(cursor_index)
                            .cursor_color(cursor_color)
                            .highlight_color(selection_highlight)
                            .color(shown_color)
                            .text_align(self.text_align)
                            .maybe(!self.multiline, |el| el.max_lines(1))
                            .highlights(text_selection.map(|h| vec![h]))
                            .maybe(display_placeholder, |el| {
                                el.span(self.placeholder.as_deref().unwrap_or_default().to_string())
                            })
                            .maybe(!display_placeholder, |el| {
                                let editor = editable.editor().read();
                                if editor.has_preedit() {
                                    let (b, p, a) = editor.preedit_text_segments();
                                    el.span(b)
                                        .span(Span::new(p).text_decoration(TextDecoration::Underline))
                                        .span(a)
                                } else {
                                    el.span(editor.rope().to_string())
                                }
                            }),
                    ),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_word_back_matches_fork_behavior() {
        assert_eq!(delete_word_back_range("hello world", 11), Some(6..11));
        assert_eq!(delete_word_back_range("hello world   ", 14), Some(6..14));
        assert_eq!(delete_word_back_range("foo/bar", 7), Some(4..7));
        assert_eq!(delete_word_back_range("foo/", 4), Some(3..4));
        assert_eq!(delete_word_back_range("line1\n", 6), Some(5..6));
        assert_eq!(delete_word_back_range("line1\nword", 10), Some(6..10));
        assert_eq!(delete_word_back_range("", 0), None);
    }

    #[test]
    fn delete_line_back_stops_at_newline_and_joins_at_line_start() {
        assert_eq!(delete_line_back_range("hello world", 11), Some(0..11));
        assert_eq!(delete_line_back_range("first\nsecond", 12), Some(6..12));
        assert_eq!(delete_line_back_range("first\nsecond", 6), Some(5..6));
        assert_eq!(delete_line_back_range("hello", 0), None);
    }

    #[test]
    fn apply_ctrl_delete_chords_update_rope_editor() {
        let mut ed = RopeEditor::new(
            "hello world".to_string(),
            TextSelection::new_cursor(11),
            4,
            EditorHistory::new(Duration::ZERO),
        );
        // Ctrl+W deletes "world"
        assert_eq!(
            apply_ctrl_delete_chord(&mut ed, &Key::Character("w".into()), Code::KeyW, Modifiers::CONTROL),
            Some(true)
        );
        assert_eq!(ed.committed_text(), "hello ");

        // Ctrl+H deletes trailing space
        assert_eq!(
            apply_ctrl_delete_chord(&mut ed, &Key::Character("h".into()), Code::KeyH, Modifiers::CONTROL),
            Some(true)
        );
        assert_eq!(ed.committed_text(), "hello");

        // Ctrl+U deletes rest of line
        assert_eq!(
            apply_ctrl_delete_chord(&mut ed, &Key::Character("u".into()), Code::KeyU, Modifiers::CONTROL),
            Some(true)
        );
        assert_eq!(ed.committed_text(), "");
    }
}
