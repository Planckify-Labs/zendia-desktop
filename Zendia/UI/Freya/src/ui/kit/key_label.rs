/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Key notation parser (`<C-j>`, `<S-Tab>`, `Ctrl+T`, `<leader>ff`) and the [`KeyLabel`] key-cap component.

use crate::ui::theme::{ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;

/// Single rendered key cap parsed from a shortcut or vim key notation string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyCap {
    pub text: String,
    pub is_modifier: bool,
}

impl KeyCap {
    pub fn modifier(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_modifier: true,
        }
    }

    pub fn key(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_modifier: false,
        }
    }
}

fn format_named_key(token: &str) -> String {
    match token.to_ascii_lowercase().as_str() {
        "cr" | "enter" | "return" => "↵".to_string(),
        "esc" | "escape" => "Esc".to_string(),
        "space" | "spc" => "Space".to_string(),
        "tab" => "Tab".to_string(),
        "bs" | "backspace" => "⌫".to_string(),
        "del" | "delete" => "Del".to_string(),
        "up" | "arrowup" => "↑".to_string(),
        "down" | "arrowdown" => "↓".to_string(),
        "left" | "arrowleft" => "←".to_string(),
        "right" | "arrowright" => "→".to_string(),
        "home" => "Home".to_string(),
        "end" => "End".to_string(),
        "pageup" | "pgup" => "PgUp".to_string(),
        "pagedown" | "pgdn" => "PgDn".to_string(),
        "leader" => "Space".to_string(),
        "bar" => "|".to_string(),
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        _ => token.to_string(),
    }
}

fn format_modifier(token: &str) -> Option<&'static str> {
    match token.to_ascii_lowercase().as_str() {
        "c" | "ctrl" | "control" => Some("Ctrl"),
        "s" | "shift" => Some("Shift"),
        "m" | "a" | "alt" | "opt" | "option" => Some("Alt"),
        "d" | "cmd" | "meta" | "super" => Some("Super"),
        _ => None,
    }
}

fn parse_angle_token(inner: &str, out: &mut Vec<KeyCap>) {
    let parts: Vec<&str> = inner.split('-').collect();
    if parts.len() == 1 {
        out.push(KeyCap::key(format_named_key(parts[0])));
        return;
    }

    let mut i = 0;
    while i + 1 < parts.len() {
        if let Some(mod_label) = format_modifier(parts[i]) {
            out.push(KeyCap::modifier(mod_label));
            i += 1;
        } else {
            break;
        }
    }
    let remainder = parts[i..].join("-");
    if !remainder.is_empty() {
        out.push(KeyCap::key(format_named_key(&remainder)));
    }
}

/// Parses Vim-style (`<C-j>`, `<C-S-Tab>`, `<leader>ff`, `gg`) or standard (`Ctrl+T`, `Alt+Left`) shortcut strings
/// into a sequence of key caps.
pub fn parse_key_notation(notation: &str) -> Vec<KeyCap> {
    let trimmed = notation.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // Standard `Ctrl+Shift+P` style when no `<...>` angle tokens are present.
    if !trimmed.contains('<') && trimmed.contains('+') && !trimmed.contains(' ') {
        let parts: Vec<&str> = trimmed.split('+').filter(|part| !part.is_empty()).collect();
        if parts.len() > 1 && format_modifier(parts[0]).is_some() {
            let mut caps = Vec::with_capacity(parts.len());
            for (idx, part) in parts.iter().enumerate() {
                if idx + 1 < parts.len()
                    && let Some(mod_label) = format_modifier(part)
                {
                    caps.push(KeyCap::modifier(mod_label));
                } else {
                    caps.push(KeyCap::key(format_named_key(part)));
                }
            }
            return caps;
        }
    }

    let mut caps = Vec::new();
    let chars: Vec<char> = trimmed.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        if chars[i] == '<'
            && let Some(rel_end) = chars[i + 1..].iter().position(|&c| c == '>')
        {
            let inner: String = chars[i + 1..i + 1 + rel_end].iter().collect();
            if !inner.is_empty() {
                parse_angle_token(&inner, &mut caps);
                i += rel_end + 2;
                continue;
            }
        }
        caps.push(KeyCap::key(chars[i].to_string()));
        i += 1;
    }
    caps
}

/// Renders parsed key notation (such as `<C-j>` or `Ctrl+T`) as themed monospace key caps.
#[derive(Clone, PartialEq)]
pub struct KeyLabel {
    notation: String,
    active: bool,
    subtle: bool,
    key: DiffKey,
}

impl KeyExt for KeyLabel {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl KeyLabel {
    pub fn new(notation: impl Into<String>) -> Self {
        Self {
            notation: notation.into(),
            active: false,
            subtle: false,
            key: DiffKey::None,
        }
    }

    /// Highlights the key cap in full hint strength (e.g. on the active picker row).
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Renders the shortcut in Zen's subtle neutral pill style (used on command palette rows).
    pub fn subtle(mut self, subtle: bool) -> Self {
        self.subtle = subtle;
        self
    }
}

impl Component for KeyLabel {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let active_anim = use_animation_with_dependencies(&self.active, move |conf, active| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *active { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::PICKER_KEY_FADE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let caps = parse_key_notation(&self.notation);

        if self.subtle {
            let joined = caps
                .into_iter()
                .map(|c| c.text.to_ascii_uppercase())
                .collect::<Vec<_>>()
                .join("+");
            let (bg, border_color, fg) = if self.active {
                (
                    Color::from_argb(230, 255, 255, 255),
                    Color::from_argb(120, 255, 255, 255),
                    theme.color(ThemeRole::Accent),
                )
            } else if theme.scheme.is_dark() {
                (
                    Color::from_argb(18, 255, 255, 255),
                    Color::from_argb(28, 255, 255, 255),
                    Color::from_argb(190, 255, 255, 255),
                )
            } else {
                (
                    Color::from_argb(14, 0, 0, 0),
                    Color::from_argb(28, 0, 0, 0),
                    Color::from_argb(185, 20, 22, 32),
                )
            };
            return rect()
                .height(Size::px(22.0))
                .padding(Gaps::new(0.0, 7.0, 0.0, 7.0))
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .corner_radius(CornerRadius::new_all(4.0))
                .background(bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(border_color),
                )
                .child(
                    label()
                        .text(joined)
                        .font_size(10.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(fg),
                );
        }

        let active_t = active_anim.get().value().clamp(0.0, 1.0);
        let hint_rgba = theme.rgba(ThemeRole::Hint);
        let hint_text_rgba = theme.rgba(ThemeRole::HintText);
        let text_rgba = theme.rgba(ThemeRole::Text);
        let idle_bg = hint_rgba.with_alpha(0.20);
        let idle_border = hint_rgba.with_alpha(0.50);
        let idle_fg = hint_rgba.mix(text_rgba, 0.45);
        let bg = idle_bg.mix(hint_rgba, active_t).to_color();
        let border_color = idle_border.mix(hint_rgba, active_t).to_color();
        let fg = idle_fg.mix(hint_text_rgba, active_t).to_color();

        let items = caps.into_iter().enumerate().map(|(index, cap)| {
            let cap_bg = if cap.is_modifier && !self.active {
                theme.rgba(ThemeRole::SurfaceRaised).with_alpha(0.75).to_color()
            } else {
                bg
            };
            rect()
                .key(index)
                .min_width(Size::px(18.0))
                .height(Size::px(18.0))
                .padding(Gaps::new(0.0, 5.0, 0.0, 5.0))
                .main_align(Alignment::center())
                .cross_align(Alignment::center())
                .corner_radius(CornerRadius::new_all(6.0))
                .background(cap_bg)
                .border(
                    Border::new()
                        .width(0.5)
                        .alignment(BorderAlignment::Inner)
                        .fill(border_color),
                )
                .child(
                    label()
                        .text(cap.text)
                        .font_size(theme.font_sizes.sm)
                        .font_weight(FontWeight::BOLD)
                        .font_family("monospace")
                        .color(fg),
                )
        });

        rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(theme.spacing.xs)
            .children(items)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vim_modified_keys() {
        assert_eq!(
            parse_key_notation("<C-j>"),
            vec![KeyCap::modifier("Ctrl"), KeyCap::key("j")]
        );
        assert_eq!(
            parse_key_notation("<C-S-Tab>"),
            vec![KeyCap::modifier("Ctrl"), KeyCap::modifier("Shift"), KeyCap::key("Tab")]
        );
        assert_eq!(
            parse_key_notation("<M-Left>"),
            vec![KeyCap::modifier("Alt"), KeyCap::key("←")]
        );
        assert_eq!(
            parse_key_notation("<D-k>"),
            vec![KeyCap::modifier("Super"), KeyCap::key("k")]
        );
    }

    #[test]
    fn parses_special_keys_and_chords() {
        assert_eq!(parse_key_notation("<CR>"), vec![KeyCap::key("↵")]);
        assert_eq!(parse_key_notation("<Esc>"), vec![KeyCap::key("Esc")]);
        assert_eq!(
            parse_key_notation("<leader>ff"),
            vec![KeyCap::key("Space"), KeyCap::key("f"), KeyCap::key("f")]
        );
        assert_eq!(parse_key_notation("g g"), vec![KeyCap::key("g"), KeyCap::key("g")]);
    }

    #[test]
    fn parses_standard_plus_shortcuts() {
        assert_eq!(
            parse_key_notation("Ctrl+T"),
            vec![KeyCap::modifier("Ctrl"), KeyCap::key("T")]
        );
        assert_eq!(
            parse_key_notation("Alt+ArrowLeft"),
            vec![KeyCap::modifier("Alt"), KeyCap::key("←")]
        );
        assert_eq!(parse_key_notation(""), Vec::<KeyCap>::new());
    }
}
