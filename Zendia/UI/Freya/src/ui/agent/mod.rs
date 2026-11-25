/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! AI agent panel, Markdown renderer, working motion, interactive tables, declared UI cards, confirmation card,
//! model & effort popover, and instructions editor.

pub mod agents_panel;
pub mod confirm_card;
pub mod follow;
pub mod instructions;
pub mod markdown;
pub mod model_card;
pub mod motion;
pub mod panel;
pub mod ui_view;

pub use agents_panel::AgentsPanel;
pub use confirm_card::AgentConfirmCard;
pub use instructions::AgentInstructionsPage;
pub use markdown::MarkdownView;
pub use model_card::AgentModelCard;
pub use motion::{RollingText, WorkingRing, WorkingRow};
pub use panel::AgentPanel;
pub use ui_view::{AgentDeclaredPageCard, AgentTableCard};

use crate::ui::kit::Icon;
use crate::ui::theme::{ResolvedScheme, ResolvedTheme, ThemeRole};
use crate::ui::vim::picker::agent_emblem_svg;
use freya::prelude::*;

const CHEVRON_DOWN_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><path d="M2.5 4.25L6 7.75l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;
const CHEVRON_RIGHT_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><path d="M4.25 2.5L7.75 6l-3.5 3.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;
const CHEVRON_LEFT_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><path d="M7.75 2.5L4.25 6l3.5 3.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;
const CHEVRON_UP_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><path d="M2.5 7.75L6 4.25l3.5 3.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;
const PAPERCLIP_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M10.8 4.4L5.8 9.4a1.6 1.6 0 0 0 2.3 2.3l5.3-5.3a2.9 2.9 0 0 0-4.1-4.1L3.7 7.9a4.2 4.2 0 0 0 5.9 5.9l4.6-4.6" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

fn inline_svg_icon(key: &'static str, bytes: &'static [u8], size: f32, color: Color) -> Element {
    SvgViewer::new((key, Bytes::from_static(bytes)))
        .width(Size::px(size))
        .height(Size::px(size))
        .color(color)
        .into_element()
}

pub fn chevron_down_icon(size: f32, color: Color) -> Element {
    inline_svg_icon("zendia-agent-chevron-down", CHEVRON_DOWN_SVG, size, color)
}

pub fn chevron_right_icon(size: f32, color: Color) -> Element {
    inline_svg_icon("zendia-agent-chevron-right", CHEVRON_RIGHT_SVG, size, color)
}

pub fn chevron_left_icon(size: f32, color: Color) -> Element {
    inline_svg_icon("zendia-agent-chevron-left", CHEVRON_LEFT_SVG, size, color)
}

pub fn chevron_up_icon(size: f32, color: Color) -> Element {
    inline_svg_icon("zendia-agent-chevron-up", CHEVRON_UP_SVG, size, color)
}

pub fn paperclip_icon(size: f32, color: Color) -> Element {
    inline_svg_icon("zendia-agent-paperclip", PAPERCLIP_SVG, size, color)
}

/// Builds Zendia-themed colors for Freya [`Input`] controls inside the agent panel so dark and light themes never
/// fall back to Freya's bright white default box.
pub fn agent_input_colors(theme: &ResolvedTheme, transparent: bool) -> InputColorsThemePartial {
    let bg = if transparent {
        Color::TRANSPARENT
    } else {
        match theme.scheme {
            ResolvedScheme::Dark => Color::from_argb(18, 255, 255, 255),
            ResolvedScheme::Light => Color::from_argb(10, 20, 24, 38),
        }
    };
    let border = Color::TRANSPARENT;
    let focus_border = if transparent {
        Color::TRANSPARENT
    } else {
        theme.rgba(ThemeRole::Accent).with_alpha(0.45).to_color()
    };
    InputColorsThemePartial {
        background: Some(Preference::Specific(bg)),
        focus_background: Some(Preference::Specific(bg)),
        border_fill: Some(Preference::Specific(border)),
        focus_border_fill: Some(Preference::Specific(focus_border)),
        color: Some(Preference::Specific(theme.color(ThemeRole::Text))),
        placeholder_color: Some(Preference::Specific(theme.color(ThemeRole::TextMuted))),
    }
}

/// Builds compact padding and corner radius overrides for Freya [`Input`] controls inside the agent panel.
pub fn agent_input_layout(padding: Gaps, radius: f32) -> InputLayoutThemePartial {
    InputLayoutThemePartial {
        corner_radius: Some(Preference::Specific(CornerRadius::new_all(radius))),
        padding: Some(Preference::Specific(padding)),
    }
}

/// Maps an [`zendia_browser_core::model::AgentId`] string to its embedded emblem identifier in `agent_emblem_svg`.
pub fn agent_emblem_id(agent_id: &str) -> &'static str {
    match agent_id {
        "claude-code" | "claude" | "agent-claude" => "agent-claude",
        "codex" | "agent-codex" => "agent-codex",
        "cursor" | "agent-cursor" => "agent-cursor",
        "antigravity" | "agent-antigravity" => "agent-antigravity",
        "opencode" | "agent-opencode" => "agent-opencode",
        "kiro" | "agent-kiro" => "agent-kiro",
        _ => "agent-claude",
    }
}

/// Renders an agent's vector emblem at `size` logical pixels, falling back to a sparkles icon if unknown.
pub fn agent_logo_element(agent_id: &str, size: f32, dimmed: bool) -> Element {
    let emblem = agent_emblem_id(agent_id);
    if let Some((key, bytes)) = agent_emblem_svg(emblem, dimmed) {
        SvgViewer::new((key, bytes))
            .width(Size::px(size))
            .height(Size::px(size))
            .into_element()
    } else {
        Icon::new("sparkles")
            .size(size)
            .role(if dimmed {
                ThemeRole::TextMuted
            } else {
                ThemeRole::Accent
            })
            .into_element()
    }
}

/// Renders an agent emblem centered inside a crisp white badge (circular or squircle) matching Zendia's chrome.
pub fn agent_badge_emblem(agent_id: &str, badge_size: f32, logo_size: f32, radius: f32, glow: bool) -> Element {
    let mut badge = rect()
        .width(Size::px(badge_size))
        .height(Size::px(badge_size))
        .corner_radius(CornerRadius::new_all(radius))
        .background(Color::WHITE)
        .center()
        .child(agent_logo_element(agent_id, logo_size, false));
    if glow {
        badge = badge.shadow(
            Shadow::new()
                .y(2.0)
                .blur(16.0)
                .color(Color::from_argb(46, 255, 220, 235)),
        );
    } else {
        badge = badge.shadow(Shadow::new().y(1.0).blur(3.0).color(Color::from_argb(30, 0, 0, 0)));
    }
    badge.into_element()
}
