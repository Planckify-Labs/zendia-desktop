/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Keymap gallery section listing the compiled standard profile from `Libraries/Keymap` with [`KeyLabel`] badges and
//! live command invocation.

use crate::gallery::sections::demo_card;
use crate::ui::core_view::use_core;
use crate::ui::kit::{KeyLabel, ListRow, Pill};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_keymap::{Binding, KeyTable, Profile};
use zendia_module_api::ui::UiTone;

#[derive(Clone, PartialEq)]
pub struct KeymapSection {
    pub state_id: String,
}

fn binding_matches_filter(binding: &Binding, state_id: &str) -> bool {
    match state_id {
        "navigation" => {
            binding.invocation.command.starts_with("tab.")
                || binding.invocation.command.starts_with("navigation.")
                || binding.invocation.command == "location.focus"
        }
        "debug" => binding.invocation.command.starts_with("debug."),
        _ => true,
    }
}

fn format_binding_detail(binding: &Binding) -> String {
    let args = binding.invocation.args_as_pairs();
    if args.is_empty() {
        binding.invocation.command.clone()
    } else {
        let joined = args
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{} ({joined})", binding.invocation.command)
    }
}

impl Component for KeymapSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut cursor = use_state(|| 0_usize);

        let profile = use_hook(Profile::standard);
        let table = use_hook(KeyTable::standard);
        let bindings: Vec<Binding> = table
            .bindings()
            .iter()
            .filter(|binding| binding_matches_filter(binding, &self.state_id))
            .cloned()
            .collect();

        let header_pills = rect()
            .direction(Direction::Horizontal)
            .content(Content::wrap())
            .spacing(theme.spacing.md)
            .child(
                Pill::new(format!("Profile: {}", profile.name))
                    .icon("terminal")
                    .tone(UiTone::Accent),
            )
            .child(Pill::new(format!("Leader: {}", profile.leader)).tone(UiTone::Neutral))
            .child(Pill::new(format!("{} bindings shown", bindings.len())).tone(UiTone::Success));

        let rows = bindings.into_iter().enumerate().map(|(index, binding)| {
            let core = core.clone();
            let command_name = binding.invocation.command.clone();
            let command_args = binding.invocation.args_as_pairs();
            let key_text = binding.keys.to_string();
            let label_text = binding
                .description
                .clone()
                .unwrap_or_else(|| binding.invocation.command.clone());
            let detail_text = format_binding_detail(&binding);

            ListRow::new(label_text)
                .key(index)
                .detail(detail_text)
                .key_hint(key_text)
                .cursor(cursor() == index)
                .on_press(move |_| {
                    cursor.set(index);
                    match Command::from_name_and_args(&command_name, &command_args) {
                        Ok(command) => {
                            core.invoke_or_log(command);
                        }
                        Err(error) => {
                            eprintln!("zendia-gallery: failed to parse `{command_name}`: {error}");
                        }
                    }
                })
        });

        ScrollView::new()
            .width(Size::fill())
            .height(Size::fill())
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new_all(theme.spacing.xxl))
                    .spacing(theme.spacing.xxl)
                    .child(demo_card(
                        "Standard Keymap Table",
                        "Loaded from Zendia/Base/res/keyboard/standard.toml. Click any row or press a shortcut directly to invoke its command through the stub backend.",
                        rect()
                            .width(Size::fill())
                            .spacing(theme.spacing.lg)
                            .child(header_pills)
                            .child(
                                rect()
                                    .width(Size::fill())
                                    .direction(Direction::Horizontal)
                                    .cross_align(Alignment::center())
                                    .spacing(theme.spacing.sm)
                                    .child(
                                        label()
                                            .text("Try pressing")
                                            .font_size(theme.font_sizes.xs)
                                            .color(theme.color(ThemeRole::TextMuted)),
                                    )
                                    .child(KeyLabel::new("<C-t>"))
                                    .child(KeyLabel::new("<C-A-m>"))
                                    .child(
                                        label()
                                            .text("anywhere in the gallery window.")
                                            .font_size(theme.font_sizes.xs)
                                            .color(theme.color(ThemeRole::TextMuted)),
                                    ),
                            )
                            .child(rect().width(Size::fill()).spacing(theme.spacing.xs).children(rows)),
                    )),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keymap_filters_partition_standard_table() {
        let table = KeyTable::standard();
        let all_count = table
            .bindings()
            .iter()
            .filter(|b| binding_matches_filter(b, "all"))
            .count();
        let nav_count = table
            .bindings()
            .iter()
            .filter(|b| binding_matches_filter(b, "navigation"))
            .count();
        let debug_count = table
            .bindings()
            .iter()
            .filter(|b| binding_matches_filter(b, "debug"))
            .count();
        assert_eq!(all_count, table.bindings().len());
        assert!(nav_count > 0);
        assert!(debug_count > 0);
        assert!(nav_count + debug_count <= all_count);
    }
}
