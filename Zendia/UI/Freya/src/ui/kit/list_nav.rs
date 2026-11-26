/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Shared standard keyboard list navigation, type-to-jump prefix search, and scroll-into-view helpers.
//!
//! Handles standard keys (`ArrowDown`, `ArrowUp`, `ArrowLeft`, `ArrowRight`, `Home`, `End`, `PageUp`, `PageDown`,
//! `Enter`, `Space`, `Escape`, and printable typeahead prefix matching) and computes scroll offsets for Freya's
//! [`ScrollController`].

use freya::prelude::*;
use std::time::{Duration, Instant};

/// Time window during which consecutive typed characters extend the same typeahead prefix.
pub const TYPEAHEAD_TIMEOUT: Duration = Duration::from_millis(700);

/// Configuration controlling which standard keys [`classify_list_key`] recognizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListNavConfig {
    /// Number of items skipped by `PageUp` / `PageDown`.
    pub page_size: usize,
    /// Whether `Space` triggers [`ListNavAction::Activate`] (true for `<select>` dropdowns).
    pub space_activates: bool,
    /// Whether `ArrowRight` / `ArrowLeft` produce [`ListNavAction::OpenSubmenu`] / [`ListNavAction::CloseSubmenu`].
    pub submenus: bool,
    /// Whether printable non-modified characters trigger [`ListNavAction::Typeahead`].
    /// Must be `false` when the list is paired with a text input field (such as the floating address bar).
    pub typeahead: bool,
    /// Whether stepping past the ends wraps around.
    pub wrap: bool,
}

impl Default for ListNavConfig {
    fn default() -> Self {
        Self {
            page_size: 8,
            space_activates: false,
            submenus: false,
            typeahead: true,
            wrap: true,
        }
    }
}

impl ListNavConfig {
    /// Preset for menus and context menus (submenus + typeahead, no Space activate).
    pub fn menu() -> Self {
        Self {
            page_size: 8,
            space_activates: false,
            submenus: true,
            typeahead: true,
            wrap: true,
        }
    }

    /// Preset for `<select>` dropdown lists (Space activates + typeahead, no submenus).
    pub fn select() -> Self {
        Self {
            page_size: 8,
            space_activates: true,
            submenus: false,
            typeahead: true,
            wrap: false,
        }
    }

    /// Preset for lists paired with a text input (address bar results, Actions palette): arrows, Home/End/PageUp/Down
    /// only — never consumes typed characters.
    pub fn with_text_input() -> Self {
        Self {
            page_size: 5,
            space_activates: false,
            submenus: false,
            typeahead: false,
            wrap: true,
        }
    }
}

/// High-level navigation intent decoded from a keyboard event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListNavAction {
    MoveNext,
    MovePrev,
    PageDown(usize),
    PageUp(usize),
    First,
    Last,
    OpenSubmenu,
    CloseSubmenu,
    Activate,
    Dismiss,
    Typeahead(char),
}

/// Decodes a [`KeyboardEventData`] into a [`ListNavAction`] under `config`.
pub fn classify_list_key(e: &KeyboardEventData, config: ListNavConfig) -> Option<ListNavAction> {
    let has_shortcut_mod = e
        .modifiers
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META);
    if has_shortcut_mod {
        return None;
    }

    match &e.key {
        Key::Named(NamedKey::Escape) => Some(ListNavAction::Dismiss),
        Key::Named(NamedKey::ArrowDown) => Some(ListNavAction::MoveNext),
        Key::Named(NamedKey::ArrowUp) => Some(ListNavAction::MovePrev),
        Key::Named(NamedKey::PageDown) => Some(ListNavAction::PageDown(config.page_size.max(1))),
        Key::Named(NamedKey::PageUp) => Some(ListNavAction::PageUp(config.page_size.max(1))),
        Key::Named(NamedKey::Home) => Some(ListNavAction::First),
        Key::Named(NamedKey::End) => Some(ListNavAction::Last),
        Key::Named(NamedKey::ArrowRight) if config.submenus => Some(ListNavAction::OpenSubmenu),
        Key::Named(NamedKey::ArrowLeft) if config.submenus => Some(ListNavAction::CloseSubmenu),
        Key::Named(NamedKey::Enter) => Some(ListNavAction::Activate),
        Key::Character(ch) if ch == " " && config.space_activates => Some(ListNavAction::Activate),
        Key::Character(ch) if config.typeahead => {
            let mut chars = ch.chars();
            let c = chars.next()?;
            if chars.next().is_none() && !c.is_control() && !c.is_whitespace() {
                Some(ListNavAction::Typeahead(c))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Steps from `current` by `delta` (`+1` or `-1`) across `len` items, skipping items where `is_selectable` is false.
pub fn step_selectable(
    len: usize,
    current: Option<usize>,
    delta: isize,
    wrap: bool,
    is_selectable: impl Fn(usize) -> bool,
) -> Option<usize> {
    if len == 0 || delta == 0 {
        return None;
    }
    let dir = delta.signum();
    if wrap {
        let start = match current {
            Some(idx) => ((idx as isize + dir).rem_euclid(len as isize)) as usize,
            None => {
                if dir > 0 {
                    0
                } else {
                    len - 1
                }
            }
        };
        for step in 0..len {
            let idx = ((start as isize + dir * step as isize).rem_euclid(len as isize)) as usize;
            if is_selectable(idx) {
                return Some(idx);
            }
        }
        None
    } else {
        let start: isize = match current {
            Some(idx) => idx as isize + dir,
            None => {
                if dir > 0 {
                    0
                } else {
                    len as isize - 1
                }
            }
        };
        let mut idx = start;
        while idx >= 0 && (idx as usize) < len {
            if is_selectable(idx as usize) {
                return Some(idx as usize);
            }
            idx += dir;
        }
        current.filter(|&i| i < len && is_selectable(i))
    }
}

/// Moves by `page_size` selectable items forward (`delta_pages > 0`) or backward (`delta_pages < 0`), clamping at the
/// ends of the list.
pub fn page_step_selectable(
    len: usize,
    current: Option<usize>,
    page_size: usize,
    forward: bool,
    is_selectable: impl Fn(usize) -> bool,
) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let selectable: Vec<usize> = (0..len).filter(|&i| is_selectable(i)).collect();
    if selectable.is_empty() {
        return None;
    }
    let cur_pos = current
        .and_then(|cur| selectable.iter().position(|&i| i == cur))
        .unwrap_or(if forward { 0 } else { selectable.len() - 1 });
    let target_pos = if forward {
        (cur_pos + page_size).min(selectable.len() - 1)
    } else {
        cur_pos.saturating_sub(page_size)
    };
    Some(selectable[target_pos])
}

/// Returns the first selectable index in `0..len`.
pub fn first_selectable(len: usize, is_selectable: impl Fn(usize) -> bool) -> Option<usize> {
    (0..len).find(|&i| is_selectable(i))
}

/// Returns the last selectable index in `0..len`.
pub fn last_selectable(len: usize, is_selectable: impl Fn(usize) -> bool) -> Option<usize> {
    (0..len).rev().find(|&i| is_selectable(i))
}

/// Debounced type-to-jump prefix buffer for menus and `<select>` lists.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypeaheadState {
    buffer: String,
    last_typed: Option<Instant>,
}

impl TypeaheadState {
    /// Appends `ch` (resetting the buffer after [`TYPEAHEAD_TIMEOUT`]) and returns the matching index in `0..len`.
    ///
    /// Pressing the same single character repeatedly cycles through items starting with that character.
    pub fn push_and_find<FLabel, FSel>(
        &mut self,
        ch: char,
        now: Instant,
        len: usize,
        current: Option<usize>,
        label_at: FLabel,
        is_selectable: FSel,
    ) -> Option<usize>
    where
        FLabel: Fn(usize) -> Option<String>,
        FSel: Fn(usize) -> bool,
    {
        if len == 0 {
            return None;
        }
        let expired = self
            .last_typed
            .is_none_or(|prev| now.saturating_duration_since(prev) > TYPEAHEAD_TIMEOUT);
        let lower_ch: String = ch.to_lowercase().collect();
        let repeated_single = !expired && self.buffer == lower_ch;

        if expired {
            self.buffer = lower_ch;
        } else if !repeated_single {
            self.buffer.push_str(&lower_ch);
        }
        self.last_typed = Some(now);

        let prefix = self.buffer.as_str();
        let start = if repeated_single {
            current.map_or(0, |i| (i + 1) % len)
        } else {
            current.unwrap_or(0)
        };

        for step in 0..len {
            let idx = (start + step) % len;
            if !is_selectable(idx) {
                continue;
            }
            if let Some(lbl) = label_at(idx)
                && lbl.trim_start().to_lowercase().starts_with(prefix)
            {
                return Some(idx);
            }
        }
        None
    }

    /// Whether a prefix is still being typed, so that a space belongs to it.
    pub fn is_typing(&self, now: Instant) -> bool {
        self.last_typed
            .is_some_and(|prev| now.saturating_duration_since(prev) <= TYPEAHEAD_TIMEOUT)
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.last_typed = None;
    }
}

/// The index a movement or type-to-jump `action` lands on, or `None` when the action does not move the cursor.
pub fn moved_index(
    action: &ListNavAction,
    typeahead: &mut TypeaheadState,
    len: usize,
    current: Option<usize>,
    wrap: bool,
    label_at: impl Fn(usize) -> Option<String>,
    is_selectable: impl Fn(usize) -> bool,
) -> Option<usize> {
    match *action {
        ListNavAction::MoveNext => step_selectable(len, current, 1, wrap, is_selectable),
        ListNavAction::MovePrev => step_selectable(len, current, -1, wrap, is_selectable),
        ListNavAction::PageDown(page) => page_step_selectable(len, current, page, true, is_selectable),
        ListNavAction::PageUp(page) => page_step_selectable(len, current, page, false, is_selectable),
        ListNavAction::First => first_selectable(len, is_selectable),
        ListNavAction::Last => last_selectable(len, is_selectable),
        ListNavAction::Typeahead(ch) => {
            typeahead.push_and_find(ch, Instant::now(), len, current, label_at, is_selectable)
        }
        _ => None,
    }
}

/// Computes the target Freya `ScrollController` Y offset (negative or zero: `0` at top, `-(content - viewport)` at
/// bottom) needed to bring the vertical span `[item_top, item_top + item_height]` into view.
///
/// Returns `None` when the item is already completely inside the visible viewport.
pub fn compute_scroll_into_view_y(
    current_scroll_y: i32,
    item_top: f32,
    item_height: f32,
    viewport_height: f32,
    content_height: f32,
) -> Option<i32> {
    if viewport_height <= 0.0 || content_height <= viewport_height {
        return (current_scroll_y != 0).then_some(0);
    }
    let visible_top = (-current_scroll_y) as f32;
    let visible_bottom = visible_top + viewport_height;
    let item_bottom = item_top + item_height;
    let max_scroll = (content_height - viewport_height).max(0.0);

    let desired_top = if item_top < visible_top {
        item_top
    } else if item_bottom > visible_bottom {
        item_bottom - viewport_height
    } else {
        return None;
    };

    let clamped_top = desired_top.clamp(0.0, max_scroll);
    let target_y = -(clamped_top.round() as i32);
    (target_y != current_scroll_y).then_some(target_y)
}

/// Adjusts `scroll_controller` minimally so `[item_top, item_top + item_height]` is visible within `viewport_height`.
pub fn scroll_item_into_view(
    scroll_controller: &mut ScrollController,
    item_top: f32,
    item_height: f32,
    viewport_height: f32,
    content_height: f32,
) -> bool {
    let (_, current_y): (i32, i32) = (*scroll_controller).into();
    if let Some(target_y) =
        compute_scroll_into_view_y(current_y, item_top, item_height, viewport_height, content_height)
    {
        scroll_controller.scroll_to_y(target_y)
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_and_page_skip_non_selectable_items() {
        let enabled = [true, false, true, false, true, true];
        let pred = |i: usize| enabled[i];

        assert_eq!(step_selectable(6, None, 1, true, pred), Some(0));
        assert_eq!(step_selectable(6, Some(0), 1, true, pred), Some(2));
        assert_eq!(step_selectable(6, Some(5), 1, true, pred), Some(0));
        assert_eq!(step_selectable(6, Some(5), 1, false, pred), Some(5));
        assert_eq!(step_selectable(6, Some(0), -1, true, pred), Some(5));
        assert_eq!(step_selectable(6, Some(0), -1, false, pred), Some(0));
        assert_eq!(first_selectable(6, pred), Some(0));
        assert_eq!(last_selectable(6, pred), Some(5));
        assert_eq!(page_step_selectable(6, Some(0), 2, true, pred), Some(4));
        assert_eq!(page_step_selectable(6, Some(5), 2, false, pred), Some(2));
    }

    #[test]
    fn typeahead_matches_prefix_and_cycles_single_char() {
        let labels = ["Apple", "Apricot", "Banana", "Blueberry"];
        let now = Instant::now();
        let mut ta = TypeaheadState::default();

        // Single char 'a' -> Apple (0), repeating 'a' -> Apricot (1).
        let first = ta.push_and_find('a', now, labels.len(), None, |i| Some(labels[i].into()), |_| true);
        assert_eq!(first, Some(0));
        let second = ta.push_and_find(
            'a',
            now + Duration::from_millis(100),
            labels.len(),
            first,
            |i| Some(labels[i].into()),
            |_| true,
        );
        assert_eq!(second, Some(1));

        // After timeout, multi-char prefix "bl" jumps directly to Blueberry (3).
        let later = now + Duration::from_secs(2);
        let b1 = ta.push_and_find('b', later, labels.len(), second, |i| Some(labels[i].into()), |_| true);
        assert_eq!(b1, Some(2));
        let b2 = ta.push_and_find(
            'l',
            later + Duration::from_millis(100),
            labels.len(),
            b1,
            |i| Some(labels[i].into()),
            |_| true,
        );
        assert_eq!(b2, Some(3));
    }

    #[test]
    fn compute_scroll_into_view_y_moves_only_when_outside_viewport() {
        // Viewport 200px, content 600px, currently scrolled to top (0).
        // Row at 50..80 is already visible -> None.
        assert_eq!(compute_scroll_into_view_y(0, 50.0, 30.0, 200.0, 600.0), None);
        // Row at 220..250 is below viewport -> scroll so bottom aligns at 250 -> top = 50 -> y = -50.
        assert_eq!(compute_scroll_into_view_y(0, 220.0, 30.0, 200.0, 600.0), Some(-50));
        // Currently at y = -200, row at 120..150 is above viewport -> scroll to top = 120 -> y = -120.
        assert_eq!(compute_scroll_into_view_y(-200, 120.0, 30.0, 200.0, 600.0), Some(-120));
    }
}
