/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Toasts: short messages over the page that dismiss themselves.
//!
//! Any command, plugin or agent can show one through `toast.show`. The core keeps the list; views draw it, run the
//! timeout and invoke `toast.dismiss` when it ends.

/// How long a toast stays up when its caller gives no timeout, as in Zen.
pub const DEFAULT_TOAST_TIMEOUT_MS: u64 = 2000;

/// Most toasts shown at once. Showing another drops the oldest.
pub const MAX_TOASTS: usize = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ToastId(pub u64);

/// A button inside a toast that runs a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToastAction {
    pub label: String,
    pub command: String,
    pub command_args: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
    pub id: ToastId,
    pub message: String,
    /// Optional secondary description line rendered below `message` (e.g. `"Expires in 30 days"`).
    pub description: Option<String>,
    pub action: Option<ToastAction>,
    pub timeout_ms: u64,
    /// Monotonic counter incremented when a matching active toast is reused and should bounce.
    pub bounce_seq: u32,
}

/// Messages Zen and Zendia show as toasts, by their Fluent ids in the fork.
pub mod messages {
    pub const COPIED_CURRENT_URL: &str = "Copied current URL!";
    pub const YANK_COPIED: &str = "Copied to the clipboard.";
    pub const YANK_EMPTY: &str = "Nothing yanked yet.";
    pub const BACKGROUND_TAB_OPENED: &str = "New background tab opened!";
    pub const SPLIT_VIEW_LIMIT: &str = "Can't add more panels to the split view!";
    pub const TABS_CLOSED: &str = "Tabs Closed!";
    pub const DOWNLOAD_STARTED: &str = "Download started";
}

/// Adds `toast` to `toasts`, reusing an existing toast with the same message/description (bumping `bounce_seq`)
/// or dropping the oldest ones past [`MAX_TOASTS`]. Returns `(final_toast, dropped_ids)`.
pub fn push_or_reuse_toast(toasts: &mut Vec<Toast>, toast: Toast) -> (Toast, Vec<ToastId>) {
    if let Some(existing) = toasts.iter_mut().find(|existing| {
        existing.id != toast.id && existing.message == toast.message && existing.description == toast.description
    }) {
        existing.action = toast.action;
        existing.timeout_ms = toast.timeout_ms;
        existing.bounce_seq = existing.bounce_seq.saturating_add(1);
        return (existing.clone(), Vec::new());
    }
    toasts.retain(|existing| existing.id != toast.id);
    toasts.push(toast.clone());
    let overflow = toasts.len().saturating_sub(MAX_TOASTS);
    let dropped = toasts.drain(..overflow).map(|dropped| dropped.id).collect();
    (toast, dropped)
}

/// Adds `toast` to `toasts`, dropping the oldest ones past [`MAX_TOASTS`]. Returns the ids that were dropped.
pub fn push_toast(toasts: &mut Vec<Toast>, toast: Toast) -> Vec<ToastId> {
    push_or_reuse_toast(toasts, toast).1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toast(id: u64) -> Toast {
        Toast {
            id: ToastId(id),
            message: format!("toast {id}"),
            description: None,
            action: None,
            timeout_ms: DEFAULT_TOAST_TIMEOUT_MS,
            bounce_seq: 0,
        }
    }

    #[test]
    fn pushing_past_the_limit_drops_the_oldest() {
        let mut toasts = Vec::new();
        for id in 1..=MAX_TOASTS as u64 {
            assert!(push_toast(&mut toasts, toast(id)).is_empty());
        }
        let dropped = push_toast(&mut toasts, toast(9));
        assert_eq!(dropped, vec![ToastId(1)]);
        assert_eq!(toasts.len(), MAX_TOASTS);
        assert_eq!(toasts.last().map(|t| t.id), Some(ToastId(9)));
    }

    #[test]
    fn pushing_an_existing_id_replaces_it() {
        let mut toasts = vec![toast(1), toast(2)];
        push_toast(&mut toasts, toast(1));
        assert_eq!(toasts.iter().map(|t| t.id.0).collect::<Vec<_>>(), vec![2, 1]);
    }

    #[test]
    fn duplicate_message_reuses_active_toast_and_bumps_bounce_seq() {
        let mut toasts = vec![toast(1)];
        let mut dup = toast(2);
        dup.message = "toast 1".to_string();
        let (reused, dropped) = push_or_reuse_toast(&mut toasts, dup);
        assert!(dropped.is_empty());
        assert_eq!(toasts.len(), 1);
        assert_eq!(reused.id, ToastId(1));
        assert_eq!(reused.bounce_seq, 1);
    }
}
