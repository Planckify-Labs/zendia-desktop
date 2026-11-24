/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Key notation parsing and formatting (`<C-t>`, `<C-S-Tab>`, `<A-Left>`, `<F5>`, `gt`, `<Leader>ff`).

use std::fmt;
use std::ops::{BitOr, BitOrAssign};
use std::str::FromStr;

/// Modifier keys held during a key stroke.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyModifiers(u8);

impl KeyModifiers {
    pub const NONE: Self = Self(0);
    pub const CTRL: Self = Self(1 << 0);
    pub const SHIFT: Self = Self(1 << 1);
    pub const ALT: Self = Self(1 << 2);
    pub const SUPER: Self = Self(1 << 3);

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Returns true if any non-Shift modifier (Ctrl, Alt, or Super) is held.
    pub const fn has_non_shift(self) -> bool {
        (self.0 & !Self::SHIFT.0) != 0
    }
}

impl BitOr for KeyModifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for KeyModifiers {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Logical key independent of any UI toolkit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyCode {
    Char(char),
    Enter,
    Tab,
    Space,
    Backspace,
    Delete,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    F(u8),
    Leader,
}

impl KeyCode {
    fn angle_name(self) -> Option<String> {
        let name = match self {
            Self::Char('<') => "lt",
            Self::Char('>') => "gt",
            Self::Char('|') => "Bar",
            Self::Char('\\') => "Bslash",
            Self::Char(c) => return Some(c.to_string()),
            Self::Enter => "CR",
            Self::Tab => "Tab",
            Self::Space => "Space",
            Self::Backspace => "BS",
            Self::Delete => "Del",
            Self::Escape => "Esc",
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Home => "Home",
            Self::End => "End",
            Self::PageUp => "PageUp",
            Self::PageDown => "PageDown",
            Self::Insert => "Ins",
            Self::F(n) => return Some(format!("F{n}")),
            Self::Leader => "Leader",
        };
        Some(name.to_string())
    }

    fn human_name(self) -> String {
        match self {
            Self::Char(c) => c.to_ascii_uppercase().to_string(),
            Self::Enter => "Enter".to_string(),
            Self::Tab => "Tab".to_string(),
            Self::Space => "Space".to_string(),
            Self::Backspace => "Backspace".to_string(),
            Self::Delete => "Delete".to_string(),
            Self::Escape => "Esc".to_string(),
            Self::Up => "Up".to_string(),
            Self::Down => "Down".to_string(),
            Self::Left => "Left".to_string(),
            Self::Right => "Right".to_string(),
            Self::Home => "Home".to_string(),
            Self::End => "End".to_string(),
            Self::PageUp => "PageUp".to_string(),
            Self::PageDown => "PageDown".to_string(),
            Self::Insert => "Insert".to_string(),
            Self::F(n) => format!("F{n}"),
            Self::Leader => "Leader".to_string(),
        }
    }
}

/// A single normalized key press (modifiers + logical key).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyStroke {
    pub modifiers: KeyModifiers,
    pub key: KeyCode,
}

impl KeyStroke {
    /// Creates a canonically normalized key stroke so parsed notation and platform key events compare equal.
    pub fn new(mut modifiers: KeyModifiers, mut key: KeyCode) -> Self {
        if key == KeyCode::Char(' ') {
            key = KeyCode::Space;
        }
        if let KeyCode::Char(c) = key {
            if modifiers.has_non_shift() {
                // With Ctrl/Alt/Super held, lowercase ASCII letters so <C-S-T> and <C-S-t> match identically.
                if c.is_ascii_alphabetic() {
                    key = KeyCode::Char(c.to_ascii_lowercase());
                }
            } else if modifiers.contains(KeyModifiers::SHIFT) {
                // Without Ctrl/Alt/Super, Shift is encoded directly in the character (e.g. 'G', '?').
                if c.is_ascii_alphabetic() {
                    key = KeyCode::Char(c.to_ascii_uppercase());
                }
                modifiers = modifiers.without(KeyModifiers::SHIFT);
            }
        }
        Self { modifiers, key }
    }

    /// Creates a plain unmodified character key stroke.
    pub fn char(c: char) -> Self {
        Self::new(KeyModifiers::NONE, KeyCode::Char(c))
    }

    /// Formats this stroke in human-readable form for menus and tooltips (e.g. `Ctrl+Shift+Tab`, `Alt+Left`, `F5`).
    pub fn display_human(&self) -> String {
        let mut parts = Vec::new();
        if self.modifiers.contains(KeyModifiers::CTRL) {
            parts.push("Ctrl".to_string());
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) {
            parts.push("Shift".to_string());
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            parts.push("Alt".to_string());
        }
        if self.modifiers.contains(KeyModifiers::SUPER) {
            parts.push("Super".to_string());
        }
        let key_part = match self.key {
            KeyCode::Char(c) if !self.modifiers.has_non_shift() => c.to_string(),
            other => other.human_name(),
        };
        parts.push(key_part);
        parts.join("+")
    }
}

impl fmt::Display for KeyStroke {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.is_empty()
            && let KeyCode::Char(c) = self.key
            && !matches!(c, '<' | '>' | '|' | '\\')
        {
            return write!(f, "{c}");
        }

        write!(f, "<")?;
        if self.modifiers.contains(KeyModifiers::CTRL) {
            write!(f, "C-")?;
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) {
            write!(f, "S-")?;
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            write!(f, "A-")?;
        }
        if self.modifiers.contains(KeyModifiers::SUPER) {
            write!(f, "D-")?;
        }
        if let Some(name) = self.key.angle_name() {
            write!(f, "{name}")?;
        }
        write!(f, ">")
    }
}

/// Errors produced when parsing key notation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotationError {
    Empty,
    UnclosedAngleBracket,
    EmptyAngleBracket,
    UnknownKey(String),
    UnknownModifier(String),
    InvalidFunctionKey(String),
    ExpectedSingleStroke(usize),
}

impl fmt::Display for NotationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "key sequence is empty"),
            Self::UnclosedAngleBracket => write!(f, "unclosed `<` in key notation"),
            Self::EmptyAngleBracket => write!(f, "empty `<>` in key notation"),
            Self::UnknownKey(token) => write!(f, "unknown key `{token}`"),
            Self::UnknownModifier(token) => write!(f, "unknown modifier `{token}`"),
            Self::InvalidFunctionKey(token) => write!(f, "invalid function key `{token}`"),
            Self::ExpectedSingleStroke(count) => write!(f, "expected 1 key stroke, got {count}"),
        }
    }
}

impl std::error::Error for NotationError {}

fn parse_modifier(token: &str) -> Option<KeyModifiers> {
    match token.to_ascii_lowercase().as_str() {
        "c" | "ctrl" | "control" => Some(KeyModifiers::CTRL),
        "s" | "shift" => Some(KeyModifiers::SHIFT),
        "a" | "m" | "alt" | "opt" | "option" => Some(KeyModifiers::ALT),
        "d" | "cmd" | "super" | "meta" => Some(KeyModifiers::SUPER),
        _ => None,
    }
}

fn parse_named_key(token: &str) -> Result<KeyCode, NotationError> {
    let lower = token.to_ascii_lowercase();
    let code = match lower.as_str() {
        "cr" | "enter" | "return" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "space" | "spc" => KeyCode::Space,
        "bs" | "backspace" => KeyCode::Backspace,
        "del" | "delete" => KeyCode::Delete,
        "esc" | "escape" => KeyCode::Escape,
        "up" | "arrowup" => KeyCode::Up,
        "down" | "arrowdown" => KeyCode::Down,
        "left" | "arrowleft" => KeyCode::Left,
        "right" | "arrowright" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdn" => KeyCode::PageDown,
        "ins" | "insert" => KeyCode::Insert,
        "leader" => KeyCode::Leader,
        "lt" => KeyCode::Char('<'),
        "gt" => KeyCode::Char('>'),
        "bar" => KeyCode::Char('|'),
        "bslash" | "backslash" => KeyCode::Char('\\'),
        "minus" | "dash" => KeyCode::Char('-'),
        "plus" => KeyCode::Char('+'),
        _ => {
            if let Some(rest) = lower.strip_prefix('f')
                && !rest.is_empty()
                && rest.chars().all(|c| c.is_ascii_digit())
            {
                let number: u8 = rest
                    .parse()
                    .map_err(|_| NotationError::InvalidFunctionKey(token.to_string()))?;
                if (1..=24).contains(&number) {
                    return Ok(KeyCode::F(number));
                }
                return Err(NotationError::InvalidFunctionKey(token.to_string()));
            }
            let mut chars = token.chars();
            if let (Some(ch), None) = (chars.next(), chars.next()) {
                return Ok(KeyCode::Char(ch));
            }
            return Err(NotationError::UnknownKey(token.to_string()));
        }
    };
    Ok(code)
}

fn parse_angle_token(inner: &str) -> Result<KeyStroke, NotationError> {
    if inner.is_empty() {
        return Err(NotationError::EmptyAngleBracket);
    }

    // Special case: `<C-->` or `<A-->` where `-` itself is the key after modifiers.
    if let Some(prefix) = inner.strip_suffix("--") {
        let mut modifiers = KeyModifiers::NONE;
        for part in prefix.split('-') {
            let modifier = parse_modifier(part).ok_or_else(|| NotationError::UnknownModifier(part.to_string()))?;
            modifiers |= modifier;
        }
        return Ok(KeyStroke::new(modifiers, KeyCode::Char('-')));
    }

    let parts: Vec<&str> = inner.split('-').collect();
    if parts.len() == 1 {
        let key = parse_named_key(parts[0])?;
        return Ok(KeyStroke::new(KeyModifiers::NONE, key));
    }

    let mut modifiers = KeyModifiers::NONE;
    for part in &parts[..parts.len() - 1] {
        let modifier = parse_modifier(part).ok_or_else(|| NotationError::UnknownModifier((*part).to_string()))?;
        modifiers |= modifier;
    }
    let key = parse_named_key(parts[parts.len() - 1])?;
    Ok(KeyStroke::new(modifiers, key))
}

impl FromStr for KeyStroke {
    type Err = NotationError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let sequence = KeySequence::parse(input)?;
        if sequence.len() != 1 {
            return Err(NotationError::ExpectedSingleStroke(sequence.len()));
        }
        Ok(sequence.strokes()[0])
    }
}

/// Ordered sequence of one or more key strokes (e.g. `<C-t>`, `gt`, `<Leader>ff`).
///
/// ASCII whitespace between tokens is ignored during parsing; use `<Space>` to denote the spacebar.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeySequence(Vec<KeyStroke>);

impl KeySequence {
    pub fn from_strokes(strokes: Vec<KeyStroke>) -> Result<Self, NotationError> {
        if strokes.is_empty() {
            return Err(NotationError::Empty);
        }
        Ok(Self(strokes))
    }

    /// Parses a key sequence written in Zendia/Vim notation.
    pub fn parse(input: &str) -> Result<Self, NotationError> {
        let chars: Vec<char> = input.chars().collect();
        let mut strokes = Vec::new();
        let mut i = 0;

        while i < chars.len() {
            if chars[i].is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if chars[i] == '<' {
                let Some(rel_end) = chars[i + 1..].iter().position(|&c| c == '>') else {
                    return Err(NotationError::UnclosedAngleBracket);
                };
                let inner: String = chars[i + 1..i + 1 + rel_end].iter().collect();
                strokes.push(parse_angle_token(&inner)?);
                i += rel_end + 2;
                continue;
            }
            strokes.push(KeyStroke::char(chars[i]));
            i += 1;
        }

        if strokes.is_empty() {
            return Err(NotationError::Empty);
        }
        Ok(Self(strokes))
    }

    pub fn strokes(&self) -> &[KeyStroke] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Replaces any `<Leader>` stroke in the sequence with the configured leader stroke.
    pub fn expand_leader(&self, leader: KeyStroke) -> Self {
        let strokes = self
            .0
            .iter()
            .map(|stroke| {
                if stroke.key == KeyCode::Leader && stroke.modifiers.is_empty() {
                    leader
                } else {
                    *stroke
                }
            })
            .collect();
        Self(strokes)
    }

    /// Formats this sequence in human-readable form (e.g. `Ctrl+T` or `Space f f`).
    pub fn display_human(&self) -> String {
        self.0
            .iter()
            .map(KeyStroke::display_human)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl FromStr for KeySequence {
    type Err = NotationError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse(input)
    }
}

impl fmt::Display for KeySequence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for stroke in &self.0 {
            write!(f, "{stroke}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modified_and_special_keys() {
        assert_eq!(
            KeyStroke::from_str("<C-t>").unwrap(),
            KeyStroke::new(KeyModifiers::CTRL, KeyCode::Char('t'))
        );
        assert_eq!(
            KeyStroke::from_str("<C-S-Tab>").unwrap(),
            KeyStroke::new(KeyModifiers::CTRL | KeyModifiers::SHIFT, KeyCode::Tab)
        );
        assert_eq!(
            KeyStroke::from_str("<A-Left>").unwrap(),
            KeyStroke::new(KeyModifiers::ALT, KeyCode::Left)
        );
        assert_eq!(
            KeyStroke::from_str("<M-Right>").unwrap(),
            KeyStroke::new(KeyModifiers::ALT, KeyCode::Right)
        );
        assert_eq!(
            KeyStroke::from_str("<D-l>").unwrap(),
            KeyStroke::new(KeyModifiers::SUPER, KeyCode::Char('l'))
        );
        assert_eq!(
            KeyStroke::from_str("<F5>").unwrap(),
            KeyStroke::new(KeyModifiers::NONE, KeyCode::F(5))
        );
        assert_eq!(
            KeyStroke::from_str("<C-->").unwrap(),
            KeyStroke::new(KeyModifiers::CTRL, KeyCode::Char('-'))
        );
    }

    #[test]
    fn normalizes_shift_consistently() {
        // Modified letters normalize to lowercase + SHIFT.
        assert_eq!(
            KeyStroke::from_str("<C-S-T>").unwrap(),
            KeyStroke::from_str("<C-S-t>").unwrap()
        );
        assert_eq!(
            KeyStroke::new(KeyModifiers::CTRL | KeyModifiers::SHIFT, KeyCode::Char('T')),
            KeyStroke::from_str("<C-S-t>").unwrap()
        );
        // Unmodified uppercase letters absorb SHIFT.
        assert_eq!(
            KeyStroke::new(KeyModifiers::SHIFT, KeyCode::Char('g')),
            KeyStroke::from_str("G").unwrap()
        );
        assert_eq!(
            KeyStroke::new(KeyModifiers::SHIFT, KeyCode::Char('?')),
            KeyStroke::from_str("?").unwrap()
        );
        // Named keys preserve SHIFT.
        assert_eq!(KeyStroke::from_str("<S-Tab>").unwrap().modifiers, KeyModifiers::SHIFT);
    }

    #[test]
    fn formats_and_round_trips_sequences() {
        let cases = [
            "<C-t>",
            "<C-S-Tab>",
            "<A-Left>",
            "<F5>",
            "<C-A-m>",
            "<D-[>",
            "<Esc>",
            "<CR>",
            "<Space>ff",
            "gt",
            "gT",
            "<lt><gt><Bar><Bslash>",
            "<C-S-r>",
        ];
        for case in cases {
            let parsed = KeySequence::parse(case).unwrap_or_else(|e| panic!("failed to parse `{case}`: {e}"));
            let displayed = parsed.to_string();
            assert_eq!(displayed, case, "canonical display mismatch for `{case}`");
            let reparsed = KeySequence::parse(&displayed).unwrap();
            assert_eq!(parsed, reparsed);
        }
    }

    #[test]
    fn formats_human_readable_shortcuts() {
        assert_eq!(KeySequence::parse("<C-t>").unwrap().display_human(), "Ctrl+T");
        assert_eq!(
            KeySequence::parse("<C-S-Tab>").unwrap().display_human(),
            "Ctrl+Shift+Tab"
        );
        assert_eq!(KeySequence::parse("<A-Left>").unwrap().display_human(), "Alt+Left");
        assert_eq!(KeySequence::parse("<F5>").unwrap().display_human(), "F5");
        assert_eq!(KeySequence::parse("gT").unwrap().display_human(), "g T");
    }

    #[test]
    fn expands_leader_key() {
        let seq = KeySequence::parse("<Leader>ff").unwrap();
        let expanded = seq.expand_leader(KeyStroke::new(KeyModifiers::NONE, KeyCode::Space));
        assert_eq!(expanded.to_string(), "<Space>ff");
    }

    #[test]
    fn rejects_invalid_notation() {
        assert_eq!(KeySequence::parse(""), Err(NotationError::Empty));
        assert_eq!(KeySequence::parse("   "), Err(NotationError::Empty));
        assert_eq!(KeySequence::parse("<C-t"), Err(NotationError::UnclosedAngleBracket));
        assert_eq!(KeySequence::parse("<>"), Err(NotationError::EmptyAngleBracket));
        assert!(matches!(
            KeySequence::parse("<X-t>"),
            Err(NotationError::UnknownModifier(_))
        ));
        assert!(matches!(
            KeySequence::parse("<Nope>"),
            Err(NotationError::UnknownKey(_))
        ));
        assert!(matches!(
            KeySequence::parse("<F0>"),
            Err(NotationError::InvalidFunctionKey(_))
        ));
        assert!(matches!(
            KeySequence::parse("<F25>"),
            Err(NotationError::InvalidFunctionKey(_))
        ));
    }
}
