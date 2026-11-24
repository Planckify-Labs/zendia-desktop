/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Declarative UI tree shared between modules (plugins, agents) and native UI views.
//!
//! Modules build plain trees of [`UiNode`] values without linking against any UI toolkit. The Freya view maps every
//! node directly onto Zendia's component kit and dispatches [`UiAction`] commands through the browser core.

/// Semantic theme role requested by a declared UI node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UiRole {
    Surface,
    SurfaceRaised,
    #[default]
    Text,
    TextMuted,
    Accent,
    Border,
    Danger,
    Warning,
    Success,
    Selection,
    Hint,
    WhichKeyKey,
    WhichKeyGroup,
}

/// Visual tone for pills, chips, toasts, and status badges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UiTone {
    #[default]
    Neutral,
    Accent,
    Success,
    Warning,
    Danger,
}

/// Anchor placement for a declared popover.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UiPlacement {
    Top,
    #[default]
    Bottom,
    Left,
    Right,
}

/// Motion style for a declared visibility transition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UiTransition {
    #[default]
    Fade,
    SlideUp,
    SlideDown,
    SlideLeft,
    SlideRight,
}

/// Command invoked through the browser core when a user interacts with a declared UI control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiAction {
    pub command: String,
    pub args: Vec<(String, String)>,
}

impl UiAction {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
        }
    }

    pub fn with_arg(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.args.push((key.into(), value.into()));
        self
    }

    pub fn with_args(mut self, args: impl IntoIterator<Item = (String, String)>) -> Self {
        self.args.extend(args);
        self
    }
}

/// Plain-data declarative UI node rendered by Zendia's component kit.
#[derive(Clone, Debug, PartialEq)]
pub enum UiNode {
    Column {
        spacing: f32,
        padding: f32,
        children: Vec<UiNode>,
    },
    Row {
        spacing: f32,
        padding: f32,
        children: Vec<UiNode>,
    },
    Text {
        text: String,
        role: UiRole,
        size: Option<f32>,
        bold: bool,
    },
    Icon {
        name: String,
        size: f32,
        role: UiRole,
    },
    Panel {
        glass: bool,
        padding: f32,
        children: Vec<UiNode>,
    },
    Pill {
        label: String,
        icon: Option<String>,
        tone: UiTone,
        action: Option<UiAction>,
    },
    IconButton {
        icon: String,
        label: String,
        active: bool,
        disabled: bool,
        role: UiRole,
        action: UiAction,
    },
    Chip {
        label: String,
        icon: Option<String>,
        selected: bool,
        disabled: bool,
        tone: UiTone,
        action: Option<UiAction>,
    },
    Toast {
        message: String,
        detail: Option<String>,
        tone: UiTone,
        dismiss_action: Option<UiAction>,
    },
    Popover {
        id: String,
        open: bool,
        placement: UiPlacement,
        anchor: Box<UiNode>,
        content: Vec<UiNode>,
    },
    KeyLabel {
        notation: String,
        active: bool,
    },
    ListRow {
        label: String,
        detail: Option<String>,
        icon: Option<String>,
        key_hint: Option<String>,
        cursor: bool,
        selected: bool,
        disabled: bool,
        action: Option<UiAction>,
    },
    ConfirmationCard {
        title: String,
        body: String,
        confirm_label: String,
        cancel_label: String,
        danger: bool,
        confirm_action: UiAction,
        cancel_action: Option<UiAction>,
    },
    Switch {
        label: Option<String>,
        checked: bool,
        disabled: bool,
        action: UiAction,
    },
    Slider {
        label: Option<String>,
        value: f32,
        min: f32,
        max: f32,
        step: Option<f32>,
        disabled: bool,
        action: UiAction,
    },
    Transition {
        visible: bool,
        kind: UiTransition,
        child: Box<UiNode>,
    },
}

impl UiNode {
    pub fn column(children: Vec<UiNode>) -> Self {
        Self::Column {
            spacing: 8.0,
            padding: 0.0,
            children,
        }
    }

    pub fn row(children: Vec<UiNode>) -> Self {
        Self::Row {
            spacing: 8.0,
            padding: 0.0,
            children,
        }
    }

    pub fn text(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            role: UiRole::Text,
            size: None,
            bold: false,
        }
    }

    pub fn panel(children: Vec<UiNode>) -> Self {
        Self::Panel {
            glass: true,
            padding: 12.0,
            children,
        }
    }

    pub fn icon(name: impl Into<String>, size: f32) -> Self {
        Self::Icon {
            name: name.into(),
            size,
            role: UiRole::Text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_builder_collects_command_and_args() {
        let action = UiAction::new("panel.toggle")
            .with_arg("panel_id", "zendia.agent")
            .with_arg("focus", "true");
        assert_eq!(action.command, "panel.toggle");
        assert_eq!(
            action.args,
            vec![
                ("panel_id".to_string(), "zendia.agent".to_string()),
                ("focus".to_string(), "true".to_string()),
            ]
        );
    }

    #[test]
    fn declarative_tree_composes_without_toolkit_types() {
        let tree = UiNode::panel(vec![
            UiNode::row(vec![
                UiNode::icon("chat", 16.0),
                UiNode::text("Agent permission"),
                UiNode::KeyLabel {
                    notation: "<C-j>".to_string(),
                    active: true,
                },
            ]),
            UiNode::ConfirmationCard {
                title: "Open tabs".to_string(),
                body: "Allow opening 3 background tabs?".to_string(),
                confirm_label: "Allow".to_string(),
                cancel_label: "Deny".to_string(),
                danger: false,
                confirm_action: UiAction::new("agent.confirm").with_arg("allow", "true"),
                cancel_action: Some(UiAction::new("agent.confirm").with_arg("allow", "false")),
            },
        ]);

        match tree {
            UiNode::Panel { glass, children, .. } => {
                assert!(glass);
                assert_eq!(children.len(), 2);
            }
            other => panic!("expected Panel, got {other:?}"),
        }
    }
}
