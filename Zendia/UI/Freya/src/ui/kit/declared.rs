/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Renderer mapping [`UiNode`] trees from `zendia-module-api` onto Zendia's native Freya component kit.
//!
//! Every interactive control in a declared tree carries a [`UiAction`] (command name + string args), which is
//! translated into a [`Command`] and dispatched through [`CoreView`].

use crate::ui::core_view::{CoreView, use_core};
use crate::ui::kit::controls::{IconButton, KitChip, KitSlider, KitSwitch};
use crate::ui::kit::icons::Icon;
use crate::ui::kit::key_label::KeyLabel;
use crate::ui::kit::list_row::ListRow;
use crate::ui::kit::overlay::{AnchoredPopover, ConfirmationCard, Toast};
use crate::ui::kit::panel::{GlassPanel, Pill};
use crate::ui::kit::transitions::{FadeTransition, SlideDirection, SlideTransition};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::contributions::SidePanelId;
use zendia_browser_core::model::{AgentId, ChatSiteId, ConversationId, SpaceId, SplitDirection, TabId, ThemeScheme};
use zendia_module_api::ui::{UiAction, UiNode, UiTransition};

fn find_arg<'a>(args: &'a [(String, String)], name: &str) -> Option<&'a str> {
    args.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_tab_id(args: &[(String, String)], key: &str) -> Option<TabId> {
    find_arg(args, key).and_then(|v| v.parse::<u64>().ok()).map(TabId)
}

/// Converts a declarative [`UiAction`] into a typed [`Command`] when it matches a built-in core command, or into
/// [`Command::Custom`] otherwise.
pub fn command_from_action(action: &UiAction) -> Command {
    let args = &action.args;
    match action.command.as_str() {
        "tab.new" => Command::TabNew {
            url: find_arg(args, "url").map(str::to_string),
            after: parse_tab_id(args, "after"),
            activate: find_arg(args, "activate").and_then(parse_bool).unwrap_or(true),
        },
        "tab.close" => Command::TabClose {
            tab_id: parse_tab_id(args, "tab_id"),
            force: find_arg(args, "force").and_then(parse_bool).unwrap_or(false),
        },
        "tab.activate" => match parse_tab_id(args, "tab_id") {
            Some(tab_id) => Command::TabActivate { tab_id },
            None => Command::Custom {
                name: action.command.clone(),
                args: action.args.clone(),
            },
        },
        "navigation.back" => Command::NavigationBack {
            tab_id: parse_tab_id(args, "tab_id"),
            count: find_arg(args, "count").and_then(|v| v.parse::<u32>().ok()),
        },
        "navigation.forward" => Command::NavigationForward {
            tab_id: parse_tab_id(args, "tab_id"),
            count: find_arg(args, "count").and_then(|v| v.parse::<u32>().ok()),
        },
        "navigation.reload" => Command::NavigationReload {
            tab_id: parse_tab_id(args, "tab_id"),
            bypass_cache: find_arg(args, "bypass_cache").and_then(parse_bool).unwrap_or(false),
        },
        "navigation.stop" => Command::NavigationStop {
            tab_id: parse_tab_id(args, "tab_id"),
        },
        "navigation.load_url" => match find_arg(args, "input") {
            Some(input) => Command::NavigationLoadUrl {
                tab_id: parse_tab_id(args, "tab_id"),
                input: input.to_string(),
            },
            None => Command::Custom {
                name: action.command.clone(),
                args: action.args.clone(),
            },
        },
        "space.switch" => match find_arg(args, "space_id").and_then(|v| v.parse::<u64>().ok()) {
            Some(space_id) => Command::SpaceSwitch {
                space_id: SpaceId(space_id),
            },
            None => Command::Custom {
                name: action.command.clone(),
                args: action.args.clone(),
            },
        },
        "split.add" => {
            let tab_id = parse_tab_id(args, "tab_id");
            let direction = find_arg(args, "direction").and_then(|d| match d.to_ascii_lowercase().as_str() {
                "horizontal" => Some(SplitDirection::Horizontal),
                "vertical" => Some(SplitDirection::Vertical),
                "grid" => Some(SplitDirection::Grid),
                _ => None,
            });
            match (tab_id, direction) {
                (Some(tab_id), Some(direction)) => Command::SplitAdd { tab_id, direction },
                _ => Command::Custom {
                    name: action.command.clone(),
                    args: action.args.clone(),
                },
            }
        }
        "panel.toggle" => match find_arg(args, "panel_id") {
            Some(panel_id) => Command::PanelToggle {
                panel_id: SidePanelId::new(panel_id),
            },
            None => Command::Custom {
                name: action.command.clone(),
                args: action.args.clone(),
            },
        },
        "agent.send" => match find_arg(args, "prompt") {
            Some(prompt) => Command::AgentSend {
                conversation_id: find_arg(args, "conversation_id")
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(ConversationId),
                agent_id: find_arg(args, "agent_id").map(AgentId::new),
                prompt: prompt.to_string(),
            },
            None => Command::Custom {
                name: action.command.clone(),
                args: action.args.clone(),
            },
        },
        "webchat.ask" => match (find_arg(args, "site_id"), find_arg(args, "prompt")) {
            (Some(site_id), Some(prompt)) => Command::WebChatAsk {
                site_id: ChatSiteId::new(site_id),
                prompt: prompt.to_string(),
            },
            _ => Command::Custom {
                name: action.command.clone(),
                args: action.args.clone(),
            },
        },
        "theme.set" => {
            let scheme = find_arg(args, "scheme").and_then(|s| match s.to_ascii_lowercase().as_str() {
                "system" => Some(ThemeScheme::System),
                "light" => Some(ThemeScheme::Light),
                "dark" => Some(ThemeScheme::Dark),
                _ => None,
            });
            Command::ThemeSet {
                scheme,
                overrides: None,
            }
        }
        _ => Command::Custom {
            name: action.command.clone(),
            args: action.args.clone(),
        },
    }
}

/// Appends or replaces an argument value before dispatching a control action (used by switches and sliders).
pub fn action_with_value(action: &UiAction, key: &str, value: String) -> UiAction {
    let mut next = action.clone();
    if let Some((_, existing)) = next.args.iter_mut().find(|(k, _)| k == key) {
        *existing = value;
    } else {
        next.args.push((key.to_string(), value));
    }
    next
}

fn dispatch_action(core: &CoreView, action: &UiAction) {
    core.invoke_or_log(command_from_action(action));
}

/// Renders a single [`UiNode`] into a Freya [`Element`], invoking any [`UiAction`] through `core`.
pub fn render_ui_node(node: &UiNode, core: &CoreView) -> Element {
    let theme = use_theme();
    match node {
        UiNode::Column {
            spacing,
            padding,
            children,
        } => rect()
            .direction(Direction::Vertical)
            .spacing(*spacing)
            .padding(Gaps::new_all(*padding))
            .children(children.iter().map(|child| render_ui_node(child, core)))
            .into_element(),

        UiNode::Row {
            spacing,
            padding,
            children,
        } => rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(*spacing)
            .padding(Gaps::new_all(*padding))
            .children(children.iter().map(|child| render_ui_node(child, core)))
            .into_element(),

        UiNode::Text { text, role, size, bold } => label()
            .text(text.clone())
            .font_size(size.unwrap_or(theme.font_sizes.md))
            .font_weight(if *bold {
                FontWeight::SEMI_BOLD
            } else {
                FontWeight::NORMAL
            })
            .color(theme.color(ThemeRole::from(*role)))
            .into_element(),

        UiNode::Icon { name, size, role } => Icon::new(name.clone())
            .size(*size)
            .role(ThemeRole::from(*role))
            .into_element(),

        UiNode::Panel {
            glass,
            padding,
            children,
        } => GlassPanel::new()
            .glass(*glass)
            .padding(Gaps::new_all(*padding))
            .children(children.iter().map(|child| render_ui_node(child, core)))
            .into_element(),

        UiNode::Pill {
            label,
            icon,
            tone,
            action,
        } => {
            let mut pill = Pill::new(label.clone()).tone(*tone);
            if let Some(icon) = icon {
                pill = pill.icon(icon.clone());
            }
            if let Some(action) = action.clone() {
                let core = core.clone();
                pill = pill.on_press(move |_| dispatch_action(&core, &action));
            }
            pill.into_element()
        }

        UiNode::IconButton {
            icon,
            label,
            active,
            disabled,
            role,
            action,
        } => {
            let core = core.clone();
            let action = action.clone();
            IconButton::new(icon.clone(), label.clone())
                .active(*active)
                .disabled(*disabled)
                .role(ThemeRole::from(*role))
                .on_press(move |_| dispatch_action(&core, &action))
                .into_element()
        }

        UiNode::Chip {
            label,
            icon,
            selected,
            disabled,
            tone,
            action,
        } => {
            let mut chip = KitChip::new(label.clone())
                .selected(*selected)
                .disabled(*disabled)
                .tone(*tone);
            if let Some(icon) = icon {
                chip = chip.icon(icon.clone());
            }
            if let Some(action) = action.clone() {
                let core = core.clone();
                chip = chip.on_press(move |_| dispatch_action(&core, &action));
            }
            chip.into_element()
        }

        UiNode::Toast {
            message,
            detail,
            tone,
            dismiss_action,
        } => {
            let mut toast = Toast::new(message.clone()).tone(*tone);
            if let Some(detail) = detail {
                toast = toast.detail(detail.clone());
            }
            if let Some(action) = dismiss_action.clone() {
                let core = core.clone();
                toast = toast.on_dismiss(move |_| dispatch_action(&core, &action));
            }
            toast.into_element()
        }

        UiNode::Popover {
            id,
            open,
            placement,
            anchor,
            content,
        } => AnchoredPopover::new(id.clone(), render_ui_node(anchor, core))
            .open(*open)
            .placement(*placement)
            .children(content.iter().map(|child| render_ui_node(child, core)))
            .into_element(),

        UiNode::KeyLabel { notation, active } => KeyLabel::new(notation.clone()).active(*active).into_element(),

        UiNode::ListRow {
            label,
            detail,
            icon,
            key_hint,
            cursor,
            selected,
            disabled,
            action,
        } => {
            let mut row = ListRow::new(label.clone())
                .cursor(*cursor)
                .selected(*selected)
                .disabled(*disabled);
            if let Some(detail) = detail {
                row = row.detail(detail.clone());
            }
            if let Some(icon) = icon {
                row = row.icon(icon.clone());
            }
            if let Some(hint) = key_hint {
                row = row.key_hint(hint.clone());
            }
            if let Some(action) = action.clone() {
                let core = core.clone();
                row = row.on_press(move |_| dispatch_action(&core, &action));
            }
            row.into_element()
        }

        UiNode::ConfirmationCard {
            title,
            body,
            confirm_label,
            cancel_label,
            danger,
            confirm_action,
            cancel_action,
        } => {
            let confirm_core = core.clone();
            let confirm_action = confirm_action.clone();
            let mut card = ConfirmationCard::new(title.clone(), body.clone())
                .confirm_label(confirm_label.clone())
                .cancel_label(cancel_label.clone())
                .danger(*danger)
                .on_confirm(move |_| dispatch_action(&confirm_core, &confirm_action));
            if let Some(cancel_action) = cancel_action.clone() {
                let cancel_core = core.clone();
                card = card.on_cancel(move |_| dispatch_action(&cancel_core, &cancel_action));
            }
            card.into_element()
        }

        UiNode::Switch {
            label,
            checked,
            disabled,
            action,
        } => {
            let core = core.clone();
            let action = action.clone();
            let mut switch = KitSwitch::new(*checked)
                .disabled(*disabled)
                .on_toggle(move |next: bool| {
                    dispatch_action(&core, &action_with_value(&action, "value", next.to_string()));
                });
            if let Some(label) = label {
                switch = switch.label(label.clone());
            }
            switch.into_element()
        }

        UiNode::Slider {
            label,
            value,
            min,
            max,
            step,
            disabled,
            action,
        } => {
            let core = core.clone();
            let action = action.clone();
            let mut slider = KitSlider::new(*value)
                .range(*min, *max)
                .step(*step)
                .disabled(*disabled)
                .on_change(move |next: f32| {
                    dispatch_action(&core, &action_with_value(&action, "value", format!("{next:.3}")));
                });
            if let Some(label) = label {
                slider = slider.label(label.clone());
            }
            slider.into_element()
        }

        UiNode::Transition { visible, kind, child } => {
            let rendered_child = render_ui_node(child, core);
            match kind {
                UiTransition::Fade => FadeTransition::new(*visible).child(rendered_child).into_element(),
                UiTransition::SlideUp => SlideTransition::new(*visible, SlideDirection::Up)
                    .child(rendered_child)
                    .into_element(),
                UiTransition::SlideDown => SlideTransition::new(*visible, SlideDirection::Down)
                    .child(rendered_child)
                    .into_element(),
                UiTransition::SlideLeft => SlideTransition::new(*visible, SlideDirection::Left)
                    .child(rendered_child)
                    .into_element(),
                UiTransition::SlideRight => SlideTransition::new(*visible, SlideDirection::Right)
                    .child(rendered_child)
                    .into_element(),
            }
        }
    }
}

/// Component rendering an arbitrary declarative [`UiNode`] tree using the active [`CoreView`] context.
#[derive(Clone, PartialEq)]
pub struct DeclaredUi {
    root: UiNode,
    key: DiffKey,
}

impl KeyExt for DeclaredUi {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl DeclaredUi {
    pub fn new(root: UiNode) -> Self {
        Self {
            root,
            key: DiffKey::None,
        }
    }
}

impl Component for DeclaredUi {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        render_ui_node(&self.root, &core)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_builtin_actions_to_typed_commands() {
        let toggle = UiAction::new("panel.toggle").with_arg("panel_id", "zendia.agent");
        assert_eq!(
            command_from_action(&toggle),
            Command::PanelToggle {
                panel_id: SidePanelId::new("zendia.agent")
            }
        );

        let theme = UiAction::new("theme.set").with_arg("scheme", "light");
        assert_eq!(
            command_from_action(&theme),
            Command::ThemeSet {
                scheme: Some(ThemeScheme::Light),
                overrides: None,
            }
        );

        let nav = UiAction::new("navigation.load_url")
            .with_arg("tab_id", "42")
            .with_arg("input", "https://ladybird.org/");
        assert_eq!(
            command_from_action(&nav),
            Command::NavigationLoadUrl {
                tab_id: Some(TabId(42)),
                input: "https://ladybird.org/".to_string(),
            }
        );
    }

    #[test]
    fn falls_back_to_custom_for_unknown_or_plugin_actions() {
        let plugin = UiAction::new("myplugin.run").with_arg("target", "page");
        assert_eq!(
            command_from_action(&plugin),
            Command::Custom {
                name: "myplugin.run".to_string(),
                args: vec![("target".to_string(), "page".to_string())],
            }
        );

        let updated = action_with_value(&plugin, "value", "true".to_string());
        assert_eq!(updated.args.len(), 2);
        let replaced = action_with_value(&updated, "value", "false".to_string());
        assert_eq!(replaced.args.len(), 2);
        assert_eq!(replaced.args[1], ("value".to_string(), "false".to_string()));
    }
}
