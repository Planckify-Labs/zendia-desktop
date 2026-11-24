/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Stateful key-sequence resolver against a compiled [`KeyTable`].

use crate::notation::{KeyCode, KeyModifiers, KeyStroke};
use crate::table::{CommandInvocation, KeyTable, TrieNode};

/// One next-key continuation available from the current pending prefix (for which-key UI).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NextKeyHint {
    pub stroke: KeyStroke,
    pub description: String,
    pub is_group: bool,
}

/// Result of feeding one key stroke to [`KeyResolver`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// Complete key sequence matched a command binding.
    Matched(CommandInvocation),
    /// Key stroke continues a multi-key binding; waiting for further input.
    Pending {
        keys: Vec<KeyStroke>,
        next: Vec<NextKeyHint>,
    },
    /// Active pending sequence was cancelled (e.g. via `<Esc>`).
    Cancelled,
    /// Key stroke did not match any binding and should pass through to the focused surface.
    Unmatched,
}

impl Resolution {
    /// Returns true if the keymap consumed this key stroke and it should not be forwarded to the page.
    pub fn is_consumed(&self) -> bool {
        !matches!(self, Self::Unmatched)
    }
}

/// Incremental resolver that tracks pending multi-key sequences against a [`KeyTable`].
#[derive(Clone, Debug, Default)]
pub struct KeyResolver {
    pending: Vec<KeyStroke>,
}

impl KeyResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pending key strokes accumulated so far for an in-progress multi-key sequence.
    pub fn pending(&self) -> &[KeyStroke] {
        &self.pending
    }

    /// Clears any pending multi-key prefix.
    pub fn reset(&mut self) {
        self.pending.clear();
    }

    /// Non-mutating check indicating whether `resolve(table, stroke)` would consume `stroke`.
    pub fn will_consume(&self, table: &KeyTable, stroke: KeyStroke) -> bool {
        let mut preview = self.clone();
        preview.resolve(table, stroke).is_consumed()
    }

    /// Advances the resolver with `stroke` against `table`.
    pub fn resolve(&mut self, table: &KeyTable, stroke: KeyStroke) -> Resolution {
        let escape = KeyStroke::new(KeyModifiers::NONE, KeyCode::Escape);
        if !self.pending.is_empty() && stroke == escape {
            // Allow an explicit `<prefix><Esc>` binding if defined; otherwise cancel the pending sequence.
            let mut candidate = self.pending.clone();
            candidate.push(stroke);
            if table.node_at(&candidate).is_none() {
                self.pending.clear();
                return Resolution::Cancelled;
            }
        }

        let had_pending = !self.pending.is_empty();
        self.pending.push(stroke);

        if let Some(node) = table.node_at(&self.pending) {
            return self.finish_node(node);
        }

        // Prefix failed to match. Clear it and, if we were mid-chord, try `stroke` from the root so
        // top-level shortcuts like `<C-w>` still work immediately.
        self.pending.clear();
        if had_pending && let Some(node) = table.node_at(std::slice::from_ref(&stroke)) {
            self.pending.push(stroke);
            return self.finish_node(node);
        }

        Resolution::Unmatched
    }

    fn finish_node(&mut self, node: &TrieNode) -> Resolution {
        if node.children.is_empty()
            && let Some(binding) = &node.binding
        {
            self.pending.clear();
            return Resolution::Matched(binding.invocation.clone());
        }

        let next = collect_next_hints(node);
        Resolution::Pending {
            keys: self.pending.clone(),
            next,
        }
    }
}

fn collect_next_hints(node: &TrieNode) -> Vec<NextKeyHint> {
    node.children
        .iter()
        .map(|(&stroke, child)| {
            if child.children.is_empty() {
                let description = child
                    .binding
                    .as_ref()
                    .and_then(|binding| {
                        binding
                            .description
                            .clone()
                            .or_else(|| Some(binding.invocation.command.clone()))
                    })
                    .unwrap_or_default();
                NextKeyHint {
                    stroke,
                    description,
                    is_group: false,
                }
            } else {
                NextKeyHint {
                    stroke,
                    description: format!("+{} keys", child.children.len()),
                    is_group: true,
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::ArgValue;
    use std::str::FromStr;

    #[test]
    fn resolves_standard_shortcuts_and_passes_unbound_keys() {
        let table = KeyTable::standard();
        let mut resolver = KeyResolver::new();

        let new_tab = resolver.resolve(&table, KeyStroke::from_str("<C-t>").unwrap());
        assert_eq!(
            new_tab,
            Resolution::Matched(CommandInvocation::new("tab.new").with_arg("activate", ArgValue::Bool(true)))
        );

        let hard_reload = resolver.resolve(&table, KeyStroke::from_str("<C-S-r>").unwrap());
        assert_eq!(
            hard_reload,
            Resolution::Matched(
                CommandInvocation::new("navigation.reload").with_arg("bypass_cache", ArgValue::Bool(true))
            )
        );

        let back = resolver.resolve(&table, KeyStroke::from_str("<A-Left>").unwrap());
        assert_eq!(back, Resolution::Matched(CommandInvocation::new("navigation.back")));

        let prev_tab = resolver.resolve(&table, KeyStroke::from_str("<C-S-Tab>").unwrap());
        assert_eq!(prev_tab, Resolution::Matched(CommandInvocation::new("tab.previous")));

        let f5 = resolver.resolve(&table, KeyStroke::from_str("<F5>").unwrap());
        assert_eq!(
            f5,
            Resolution::Matched(
                CommandInvocation::new("navigation.reload").with_arg("bypass_cache", ArgValue::Bool(false))
            )
        );

        let plain_a = resolver.resolve(&table, KeyStroke::char('a'));
        assert_eq!(plain_a, Resolution::Unmatched);
        assert!(resolver.pending().is_empty());
    }

    #[test]
    fn resolves_multi_key_sequences_and_next_key_hints() {
        let table = KeyTable::from_toml(
            r#"
            name = "vim-sample"
            leader = "<Space>"

            [[bindings]]
            keys = "gt"
            command = "tab.next"
            description = "Next tab"

            [[bindings]]
            keys = "gT"
            command = "tab.previous"
            description = "Previous tab"

            [[bindings]]
            keys = "<Leader>ff"
            command = "palette.open"
            description = "Find command"

            [[bindings]]
            keys = "<C-w>"
            command = "tab.close"
            description = "Close tab"
            "#,
        )
        .unwrap();

        let mut resolver = KeyResolver::new();

        assert!(resolver.will_consume(&table, KeyStroke::char('g')));
        assert!(resolver.pending().is_empty());

        let first = resolver.resolve(&table, KeyStroke::char('g'));
        match first {
            Resolution::Pending { keys, next } => {
                assert_eq!(keys, vec![KeyStroke::char('g')]);
                assert_eq!(next.len(), 2);
                assert!(next.iter().any(|h| h.stroke == KeyStroke::char('t') && !h.is_group));
                assert!(next.iter().any(|h| h.stroke == KeyStroke::char('T') && !h.is_group));
            }
            other => panic!("expected Pending, got {other:?}"),
        }

        let second = resolver.resolve(&table, KeyStroke::char('t'));
        assert_eq!(second, Resolution::Matched(CommandInvocation::new("tab.next")));
        assert!(resolver.pending().is_empty());

        // Leader group hint
        let leader = resolver.resolve(&table, KeyStroke::new(KeyModifiers::NONE, KeyCode::Space));
        match leader {
            Resolution::Pending { next, .. } => {
                assert_eq!(next.len(), 1);
                assert!(next[0].is_group);
                assert_eq!(next[0].stroke, KeyStroke::char('f'));
            }
            other => panic!("expected Pending group, got {other:?}"),
        }

        // Escape cancels pending prefix.
        let cancelled = resolver.resolve(&table, KeyStroke::new(KeyModifiers::NONE, KeyCode::Escape));
        assert_eq!(cancelled, Resolution::Cancelled);
        assert!(resolver.pending().is_empty());

        // Mismatch clears prefix and retries top-level shortcut.
        let _ = resolver.resolve(&table, KeyStroke::char('g'));
        let fallback = resolver.resolve(&table, KeyStroke::from_str("<C-w>").unwrap());
        assert_eq!(fallback, Resolution::Matched(CommandInvocation::new("tab.close")));
    }
}
