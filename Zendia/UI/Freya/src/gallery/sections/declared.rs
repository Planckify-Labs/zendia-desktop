/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Declared UI gallery section rendering sample `zendia-module-api` [`UiNode`] trees natively through [`DeclaredUi`].

use crate::gallery::sections::{demo_card, glass_backdrop};
use crate::ui::kit::DeclaredUi;
use crate::ui::theme::use_theme;
use freya::prelude::*;
use zendia_module_api::ui::{UiAction, UiNode, UiRole, UiTone, UiTransition};

#[derive(Clone, PartialEq)]
pub struct DeclaredSection {
    pub state_id: String,
}

/// Builds a pure-data declarative plugin panel tree exercising every major [`UiNode`] variant.
pub fn sample_plugin_tree() -> UiNode {
    UiNode::Panel {
        glass: true,
        padding: 16.0,
        children: vec![
            UiNode::Row {
                spacing: 10.0,
                padding: 0.0,
                children: vec![
                    UiNode::Icon {
                        name: "reader-mode".to_string(),
                        size: 18.0,
                        role: UiRole::Accent,
                    },
                    UiNode::Text {
                        text: "Reader & Notes Plugin".to_string(),
                        role: UiRole::Text,
                        size: Some(15.0),
                        bold: true,
                    },
                    UiNode::Pill {
                        label: "Sandboxed".to_string(),
                        icon: Some("security".to_string()),
                        tone: UiTone::Success,
                        action: Some(UiAction::new("plugin.reader.status")),
                    },
                    UiNode::IconButton {
                        icon: "reload".to_string(),
                        label: "Refresh plugin view".to_string(),
                        active: false,
                        disabled: false,
                        role: UiRole::TextMuted,
                        action: UiAction::new("plugin.reader.refresh"),
                    },
                ],
            },
            UiNode::Text {
                text: "Declared entirely as plain UiNode data in zendia-module-api. Clicking actions below dispatches commands through BrowserCore (handled built-ins and unhandled plugin.* commands both appear in the command log)."
                    .to_string(),
                role: UiRole::TextMuted,
                size: Some(12.0),
                bold: false,
            },
            UiNode::Row {
                spacing: 8.0,
                padding: 0.0,
                children: vec![
                    UiNode::Chip {
                        label: "Switch to Light".to_string(),
                        icon: Some("sun".to_string()),
                        selected: false,
                        disabled: false,
                        tone: UiTone::Accent,
                        action: Some(UiAction::new("theme.set").with_arg("scheme", "light")),
                    },
                    UiNode::Chip {
                        label: "Switch to Dark".to_string(),
                        icon: Some("moon".to_string()),
                        selected: false,
                        disabled: false,
                        tone: UiTone::Accent,
                        action: Some(UiAction::new("theme.set").with_arg("scheme", "dark")),
                    },
                    UiNode::Chip {
                        label: "Export Markdown".to_string(),
                        icon: Some("save".to_string()),
                        selected: true,
                        disabled: false,
                        tone: UiTone::Success,
                        action: Some(UiAction::new("plugin.reader.export").with_arg("format", "markdown")),
                    },
                ],
            },
            UiNode::Switch {
                label: Some("Auto-simplify articles on load".to_string()),
                checked: true,
                disabled: false,
                action: UiAction::new("plugin.reader.set_auto_simplify"),
            },
            UiNode::Slider {
                label: Some("Reading column width (ch)".to_string()),
                value: 68.0,
                min: 48.0,
                max: 96.0,
                step: Some(4.0),
                disabled: false,
                action: UiAction::new("plugin.reader.set_width"),
            },
            UiNode::Column {
                spacing: 4.0,
                padding: 0.0,
                children: vec![
                    UiNode::ListRow {
                        label: "Open Ladybird release notes".to_string(),
                        detail: Some("tab.new (handled)".to_string()),
                        icon: Some("new-tab".to_string()),
                        key_hint: Some("<C-t>".to_string()),
                        cursor: true,
                        selected: false,
                        disabled: false,
                        action: Some(UiAction::new("tab.new").with_arg("url", "https://ladybird.org/")),
                    },
                    UiNode::ListRow {
                        label: "Highlight current paragraph".to_string(),
                        detail: Some("plugin.reader.highlight (unhandled)".to_string()),
                        icon: Some("paintbrush".to_string()),
                        key_hint: Some("<leader>h".to_string()),
                        cursor: false,
                        selected: true,
                        disabled: false,
                        action: Some(UiAction::new("plugin.reader.highlight").with_arg("color", "yellow")),
                    },
                ],
            },
            UiNode::Transition {
                visible: true,
                kind: UiTransition::Fade,
                child: Box::new(UiNode::Toast {
                    message: "3 highlights saved for this page".to_string(),
                    detail: Some("Stored inside PluginHost's isolated data folder".to_string()),
                    tone: UiTone::Accent,
                    dismiss_action: Some(UiAction::new("plugin.reader.dismiss_notice")),
                }),
            },
        ],
    }
}

/// Builds a pure-data declarative tree representing an agent-produced summary and permission card.
pub fn sample_agent_tree() -> UiNode {
    UiNode::Panel {
        glass: false,
        padding: 16.0,
        children: vec![
            UiNode::Row {
                spacing: 10.0,
                padding: 0.0,
                children: vec![
                    UiNode::Icon {
                        name: "chat".to_string(),
                        size: 16.0,
                        role: UiRole::Accent,
                    },
                    UiNode::Text {
                        text: "Agent Tool Output (Declared UI)".to_string(),
                        role: UiRole::Text,
                        size: Some(15.0),
                        bold: true,
                    },
                    UiNode::KeyLabel {
                        notation: "<leader>a".to_string(),
                        active: true,
                    },
                ],
            },
            UiNode::Row {
                spacing: 8.0,
                padding: 0.0,
                children: vec![
                    UiNode::Pill {
                        label: "4 pages scanned".to_string(),
                        icon: Some("search".to_string()),
                        tone: UiTone::Neutral,
                        action: None,
                    },
                    UiNode::Pill {
                        label: "Waiting approval".to_string(),
                        icon: Some("warning".to_string()),
                        tone: UiTone::Warning,
                        action: None,
                    },
                ],
            },
            UiNode::ConfirmationCard {
                title: "Submit search form on docs.rs?".to_string(),
                body:
                    "The agent prepared a query for `freya 0.5.4 Portal` and requests confirmation before submitting."
                        .to_string(),
                confirm_label: "Allow Submit".to_string(),
                cancel_label: "Deny".to_string(),
                danger: false,
                confirm_action: UiAction::new("agent.confirm")
                    .with_arg("step_id", "12")
                    .with_arg("allow", "true"),
                cancel_action: Some(
                    UiAction::new("agent.confirm")
                        .with_arg("step_id", "12")
                        .with_arg("allow", "false"),
                ),
            },
        ],
    }
}

impl Component for DeclaredSection {
    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.state_id)
    }

    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let tree = if self.state_id == "agent" {
            sample_agent_tree()
        } else {
            sample_plugin_tree()
        };

        let card_title = if self.state_id == "agent" {
            "Agent Declared UI Tree"
        } else {
            "Plugin Panel Declared UI Tree"
        };
        let card_subtitle = "Rendered via DeclaredUi::new(UiNode) -> Freya component kit -> CoreView::invoke_or_log.";

        ScrollView::new().width(Size::fill()).height(Size::fill()).child(
            rect()
                .width(Size::fill())
                .padding(Gaps::new_all(theme.spacing.xxl))
                .spacing(theme.spacing.xxl)
                .child(demo_card(
                    card_title,
                    card_subtitle,
                    glass_backdrop(DeclaredUi::new(tree)),
                )),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_declared_trees_construct_cleanly() {
        let plugin = sample_plugin_tree();
        let agent = sample_agent_tree();
        assert!(matches!(plugin, UiNode::Panel { glass: true, .. }));
        assert!(matches!(agent, UiNode::Panel { glass: false, .. }));
    }
}
