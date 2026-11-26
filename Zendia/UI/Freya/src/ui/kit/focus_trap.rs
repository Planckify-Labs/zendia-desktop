/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Modal focus-trap and keyboard focus-ring helpers shared across dialogs and popovers.

use crate::ui::theme::{ResolvedTheme, ThemeRole};
use freya::prelude::*;

/// Computes the next index in a circular tab-focus ring of length `count`.
pub fn next_focus_trap_index(current: Option<usize>, count: usize, backwards: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match current {
        Some(idx) if backwards => (idx + count - 1) % count,
        Some(idx) => (idx + 1) % count,
        None if backwards => count - 1,
        None => 0,
    })
}

/// Handles `Tab` / `Shift+Tab` inside a modal dialog so focus cycles strictly through `focus_ring` and never escapes
/// to background chrome.
///
/// Returns `true` when the key event was handled as a trapped Tab move (caller should skip further handling).
pub fn handle_modal_tab_key(e: &Event<KeyboardEventData>, focus_ring: &[AccessibilityId]) -> bool {
    if e.key != Key::Named(NamedKey::Tab) {
        return false;
    }
    if e.modifiers
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META)
    {
        return false;
    }
    e.stop_propagation();
    e.prevent_default();
    let backwards = e.modifiers.contains(Modifiers::SHIFT);
    let current = focus_ring.iter().position(|id| id.is_focused());
    if let Some(next_idx) = next_focus_trap_index(current, focus_ring.len(), backwards) {
        focus_ring[next_idx].request_focus();
    }
    true
}

/// Returns a consistent inner focus ring [`Border`] when `focused` is true, or `fallback` otherwise.
pub fn focus_ring_border(focused: bool, theme: &ResolvedTheme, fallback: Border) -> Border {
    if focused {
        Border::new()
            .width(2.0)
            .alignment(BorderAlignment::Inner)
            .fill(theme.color(ThemeRole::FocusRing))
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_trap_cycles_forward_and_backward() {
        assert_eq!(next_focus_trap_index(None, 0, false), None);
        assert_eq!(next_focus_trap_index(None, 4, false), Some(0));
        assert_eq!(next_focus_trap_index(None, 4, true), Some(3));
        assert_eq!(next_focus_trap_index(Some(0), 4, false), Some(1));
        assert_eq!(next_focus_trap_index(Some(3), 4, false), Some(0));
        assert_eq!(next_focus_trap_index(Some(0), 4, true), Some(3));
        assert_eq!(next_focus_trap_index(Some(2), 4, true), Some(1));
    }
}
