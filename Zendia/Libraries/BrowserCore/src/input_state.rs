/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Vim input state, statusline profile settings, transient announcements, and which-key computation.
//!
//! Which-key entries and groups are computed directly from the active compiled [`KeyTable`] for the current
//! [`InputMode`] and sorted with which-key.nvim's ordering rules.

use crate::model::{InputMode, WhichKeyEntry, WhichKeyState};
use std::cmp::Ordering;
use std::sync::OnceLock;
use zendia_keymap::{KeySequence, KeyStroke, KeyTable, Profile};

/// Default duration in milliseconds for transient statusline announcements.
pub const DEFAULT_ANNOUNCEMENT_TIMEOUT_MS: u64 = 2200;

/// How the mode indicator rests after activity settles (Zendia's `zendia.statusline.show`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum StatuslineShow {
    /// Keep the indicator full size whenever vim mode is enabled.
    Always,
    /// Shrink to a small corner badge after the quiet timeout (default).
    #[default]
    Shrink,
    /// Hide the indicator when quiet (also accepts the fork's `"change"` alias).
    Never,
}

impl StatuslineShow {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Shrink => "shrink",
            Self::Never => "never",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "always" => Some(Self::Always),
            "shrink" => Some(Self::Shrink),
            "never" | "change" => Some(Self::Never),
            _ => None,
        }
    }
}

/// Profile settings for the vim mode statusline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatuslineSettings {
    pub show: StatuslineShow,
    pub enabled: bool,
}

impl Default for StatuslineSettings {
    fn default() -> Self {
        Self {
            show: StatuslineShow::Shrink,
            enabled: true,
        }
    }
}

/// Short transient message displayed beside the mode indicator in the statusline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatuslineAnnouncement {
    pub id: u64,
    pub message: String,
    pub timeout_ms: u64,
}

/// One computed which-key row with formatted key label, description, and optional group color token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhichKeyItem {
    pub key: String,
    pub label: String,
    pub description: String,
    pub is_group: bool,
    pub group_color: Option<String>,
}

/// Computed which-key popup payload (for a pending prefix or the all-keymaps view).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhichKeyView {
    pub title: String,
    pub items: Vec<WhichKeyItem>,
    pub is_all_keymaps: bool,
}

impl WhichKeyView {
    /// Converts this view into the model's [`WhichKeyState`] representation.
    pub fn to_which_key_state(&self) -> WhichKeyState {
        WhichKeyState {
            title: self.title.clone(),
            entries: self
                .items
                .iter()
                .map(|item| WhichKeyEntry {
                    key: item.label.clone(),
                    description: if item.is_group && !item.description.starts_with('+') {
                        format!("+{}", item.description)
                    } else {
                        item.description.clone()
                    },
                    is_group: item.is_group,
                })
                .collect(),
        }
    }
}

/// Short single-letter mode key matching the vim profile and mascot drawings.
pub fn mode_code(mode: InputMode) -> &'static str {
    match mode {
        InputMode::Normal => "n",
        InputMode::Insert => "i",
        InputMode::Visual => "v",
        InputMode::Explorer => "e",
        InputMode::Toolbar => "t",
        InputMode::Library => "l",
        InputMode::Window => "w",
    }
}

/// Uppercase human-readable mode name shown in screen-reader labels and statusline pills.
pub fn mode_name(mode: InputMode) -> &'static str {
    match mode {
        InputMode::Normal => "NORMAL",
        InputMode::Insert => "INSERT",
        InputMode::Visual => "VISUAL",
        InputMode::Explorer => "EXPLORER",
        InputMode::Toolbar => "TOOLBAR",
        InputMode::Library => "LIBRARY",
        InputMode::Window => "WINDOW",
    }
}

/// Formats a raw key token for display in which-key titles and key columns.
pub fn format_key_label(token: &str) -> String {
    match token {
        "<Space>" | "<Leader>" | "<leader>" => "SPC".to_string(),
        "<Tab>" => "TAB".to_string(),
        "<CR>" => "RET".to_string(),
        "<Esc>" => "ESC".to_string(),
        "<BS>" => "BS".to_string(),
        "<lt>" => "<".to_string(),
        "<gt>" => ">".to_string(),
        "<Bar>" => "|".to_string(),
        "<Bslash>" => "\\".to_string(),
        "<C-Bslash>" => "<C-\\>".to_string(),
        other => other.to_string(),
    }
}

/// Returns the cached embedded default vim [`Profile`].
pub fn vim_profile() -> &'static Profile {
    static PROFILE: OnceLock<Profile> = OnceLock::new();
    PROFILE.get_or_init(Profile::vim)
}

/// Returns a compiled [`KeyTable`] for `mode` from the default vim profile.
pub fn vim_table_for_mode(mode: InputMode) -> KeyTable {
    KeyTable::from_profile_for_mode(vim_profile(), mode_code(mode))
}

fn parse_prefix_strokes(profile: &Profile, tokens: &[String]) -> Option<Vec<KeyStroke>> {
    let mut strokes = Vec::with_capacity(tokens.len());
    for token in tokens {
        let parsed = KeySequence::parse(token).ok()?.expand_leader(profile.leader);
        strokes.extend_from_slice(parsed.strokes());
    }
    Some(strokes)
}

fn is_word_key(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_bracketed_key(key: &str) -> bool {
    key.starts_with('<') && key.ends_with('>') && key.len() >= 3
}

fn natural_lowercase_key(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 8);
    let bytes = key.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let digits = &key[start..i];
            for _ in digits.len()..9 {
                out.push('0');
            }
            out.push_str(digits);
        } else {
            let ch = key[i..].chars().next().unwrap();
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            i += ch.len_utf8();
        }
    }
    out
}

/// Normalizes keymap alias names inside angle brackets (e.g. `<C-Bslash>` -> `<C-\>`) so sorting matches the fork's
/// literal key tokens (`<C-\>` sorts before `<C-b>`).
fn normalize_sort_key(key: &str) -> String {
    match key {
        "<C-Bslash>" => "<C-\\>".to_string(),
        "<Bslash>" => "\\".to_string(),
        "<lt>" => "<".to_string(),
        "<gt>" => ">".to_string(),
        "<Bar>" => "|".to_string(),
        other => other.replace("-Bslash>", "-\\>"),
    }
}

/// Sorts which-key items using which-key.nvim's default order:
/// mappings before groups, word keys before symbols, bracketed keys before bare symbols,
/// natural number order, lowercase before uppercase, then raw key order.
pub fn sort_which_key_items(items: &mut [WhichKeyItem]) {
    items.sort_by(|a, b| {
        let ak = normalize_sort_key(&a.key);
        let bk = normalize_sort_key(&b.key);
        let ord = a
            .is_group
            .cmp(&b.is_group)
            .then_with(|| (!is_word_key(&ak)).cmp(&(!is_word_key(&bk))))
            .then_with(|| (!is_bracketed_key(&ak)).cmp(&(!is_bracketed_key(&bk))))
            .then_with(|| natural_lowercase_key(&ak).cmp(&natural_lowercase_key(&bk)))
            .then_with(|| {
                let a_upper = ak != ak.to_lowercase();
                let b_upper = bk != bk.to_lowercase();
                a_upper.cmp(&b_upper)
            });
        if ord != Ordering::Equal { ord } else { ak.cmp(&bk) }
    });
}

/// Builds the breadcrumb title for a pending key prefix (e.g. `SPC`, `SPC » a +ai`, `g +goto`).
pub fn breadcrumb_for_prefix(table: &KeyTable, profile: &Profile, pending_tokens: &[String]) -> String {
    let trail = pending_tokens
        .iter()
        .map(|token| format_key_label(token))
        .collect::<Vec<_>>()
        .join(" » ");
    let group_name = parse_prefix_strokes(profile, pending_tokens)
        .and_then(|strokes| table.group_for(&strokes))
        .map(|group| group.name.as_str());
    match group_name {
        Some(name) if !trail.is_empty() => format!("{trail} +{name}"),
        Some(name) => format!("+{name}"),
        None => trail,
    }
}

/// Computes the which-key items available under `prefix` in `table`.
pub fn compute_which_key_items(table: &KeyTable, prefix: &[KeyStroke]) -> Vec<WhichKeyItem> {
    let children = table.children_at(prefix);
    let mut items = Vec::with_capacity(children.len());
    let mut child_prefix = Vec::with_capacity(prefix.len() + 1);
    child_prefix.extend_from_slice(prefix);

    for child in children {
        let raw_key = child.stroke.to_string();
        let label = format_key_label(&raw_key);
        child_prefix.truncate(prefix.len());
        child_prefix.push(child.stroke);

        if child.is_group {
            let group_spec = table.group_for(&child_prefix);
            let description = group_spec
                .map(|g| g.name.clone())
                .or_else(|| child.binding.as_ref().and_then(|b| b.description.clone()))
                .unwrap_or_else(|| {
                    let count = child.descendant_count;
                    format!("{count} keymap{}", if count == 1 { "" } else { "s" })
                });
            let group_color = group_spec.and_then(|g| g.color.clone());
            items.push(WhichKeyItem {
                key: raw_key,
                label,
                description,
                is_group: true,
                group_color,
            });
        } else {
            let description = child
                .binding
                .as_ref()
                .map(|b| b.description.clone().unwrap_or_else(|| b.invocation.command.clone()))
                .unwrap_or_default();
            items.push(WhichKeyItem {
                key: raw_key,
                label,
                description,
                is_group: false,
                group_color: None,
            });
        }
    }

    sort_which_key_items(&mut items);
    items
}

/// Computes the which-key popup view for `pending_tokens` in `mode` from the active vim profile.
pub fn which_key_for_prefix(mode: InputMode, pending_tokens: &[String]) -> Option<WhichKeyView> {
    if pending_tokens.is_empty() {
        return None;
    }
    let profile = vim_profile();
    let table = KeyTable::from_profile_for_mode(profile, mode_code(mode));
    let prefix = parse_prefix_strokes(profile, pending_tokens)?;
    let items = compute_which_key_items(&table, &prefix);
    if items.is_empty() {
        return None;
    }
    Some(WhichKeyView {
        title: breadcrumb_for_prefix(&table, profile, pending_tokens),
        items,
        is_all_keymaps: false,
    })
}

/// Computes the "all keymaps" view (`<leader>?`) listing every top-level binding and group in `mode`.
pub fn which_key_all_keymaps(mode: InputMode) -> WhichKeyView {
    let table = vim_table_for_mode(mode);
    let items = compute_which_key_items(&table, &[]);
    WhichKeyView {
        title: "Keymaps".to_string(),
        items,
        is_all_keymaps: true,
    }
}

/// Resolves the configured group color token (for example `"pink"` or `"blue"`) for a group label shown in `mode`.
pub fn group_color_for_entry(mode: InputMode, pending_tokens: &[String], display_key: &str) -> Option<String> {
    let profile = vim_profile();
    let table = KeyTable::from_profile_for_mode(profile, mode_code(mode));
    let mut strokes = if pending_tokens.is_empty() {
        Vec::new()
    } else {
        parse_prefix_strokes(profile, pending_tokens)?
    };
    let raw_token = match display_key {
        "SPC" => "<Space>",
        "TAB" => "<Tab>",
        "RET" => "<CR>",
        "ESC" => "<Esc>",
        "BS" => "<BS>",
        "<" => "<lt>",
        ">" => "<gt>",
        "|" => "<Bar>",
        "\\" => "<Bslash>",
        other => other,
    };
    let next = KeySequence::parse(raw_token).ok()?.expand_leader(profile.leader);
    strokes.extend_from_slice(next.strokes());
    table.group_for(&strokes).and_then(|group| group.color.clone())
}

/// One step in the stub's `debug.cycle_input_state` sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SampleInputState {
    pub mode: InputMode,
    pub count: Option<u32>,
    pub pending_keys: Vec<String>,
    pub which_key: Option<WhichKeyState>,
    pub announcement: Option<String>,
}

/// Total number of steps in [`sample_input_state`].
pub const SAMPLE_INPUT_STEP_COUNT: usize = 11;

/// Returns deterministic sample input states covering every vim mode, counts, pending keys, and which-key popups.
///
/// Steps 1..=3 preserve the original cycle contract (Leader + which-key, subgroup, Insert) while steps 4..=10 cover
/// Visual, Explorer, Toolbar, Library, Window, Normal with count + pending `g`, and the All Keymaps view.
pub fn sample_input_state(step: usize) -> SampleInputState {
    match step % SAMPLE_INPUT_STEP_COUNT {
        1 => {
            let pending = vec!["<Space>".to_string()];
            let which_key = which_key_for_prefix(InputMode::Normal, &pending).map(|v| v.to_which_key_state());
            SampleInputState {
                mode: InputMode::Normal,
                count: None,
                pending_keys: pending,
                which_key,
                announcement: None,
            }
        }
        2 => {
            let pending = vec!["<Space>".to_string(), "a".to_string()];
            let which_key = which_key_for_prefix(InputMode::Normal, &pending).map(|v| v.to_which_key_state());
            SampleInputState {
                mode: InputMode::Normal,
                count: None,
                pending_keys: pending,
                which_key,
                announcement: None,
            }
        }
        3 => SampleInputState {
            mode: InputMode::Insert,
            count: None,
            pending_keys: Vec::new(),
            which_key: None,
            announcement: None,
        },
        4 => SampleInputState {
            mode: InputMode::Visual,
            count: None,
            pending_keys: Vec::new(),
            which_key: None,
            announcement: None,
        },
        5 => SampleInputState {
            mode: InputMode::Explorer,
            count: None,
            pending_keys: vec!["g".to_string()],
            which_key: None,
            announcement: None,
        },
        6 => SampleInputState {
            mode: InputMode::Toolbar,
            count: None,
            pending_keys: Vec::new(),
            which_key: None,
            announcement: None,
        },
        7 => SampleInputState {
            mode: InputMode::Library,
            count: None,
            pending_keys: Vec::new(),
            which_key: None,
            announcement: None,
        },
        8 => SampleInputState {
            mode: InputMode::Window,
            count: None,
            pending_keys: Vec::new(),
            which_key: None,
            announcement: None,
        },
        9 => {
            let pending = vec!["g".to_string()];
            let which_key = which_key_for_prefix(InputMode::Normal, &pending).map(|v| v.to_which_key_state());
            SampleInputState {
                mode: InputMode::Normal,
                count: Some(3),
                pending_keys: pending,
                which_key,
                announcement: None,
            }
        }
        10 => SampleInputState {
            mode: InputMode::Normal,
            count: None,
            pending_keys: Vec::new(),
            which_key: Some(which_key_all_keymaps(InputMode::Normal).to_which_key_state()),
            announcement: None,
        },
        _ => SampleInputState {
            mode: InputMode::Normal,
            count: None,
            pending_keys: Vec::new(),
            which_key: None,
            announcement: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_statusline_show_variants_and_alias() {
        assert_eq!(StatuslineShow::parse("always"), Some(StatuslineShow::Always));
        assert_eq!(StatuslineShow::parse("shrink"), Some(StatuslineShow::Shrink));
        assert_eq!(StatuslineShow::parse("never"), Some(StatuslineShow::Never));
        assert_eq!(StatuslineShow::parse("change"), Some(StatuslineShow::Never));
        assert_eq!(StatuslineShow::parse("bogus"), None);
        assert_eq!(StatuslineShow::Shrink.as_str(), "shrink");
    }

    #[test]
    fn sorts_which_key_items_like_which_key_nvim() {
        let mut items = vec![
            WhichKeyItem {
                key: "a".to_string(),
                label: "a".to_string(),
                description: "ai".to_string(),
                is_group: true,
                group_color: Some("pink".to_string()),
            },
            WhichKeyItem {
                key: "?".to_string(),
                label: "?".to_string(),
                description: "Keymaps".to_string(),
                is_group: false,
                group_color: None,
            },
            WhichKeyItem {
                key: "<Space>".to_string(),
                label: "SPC".to_string(),
                description: "Find".to_string(),
                is_group: false,
                group_color: None,
            },
            WhichKeyItem {
                key: "E".to_string(),
                label: "E".to_string(),
                description: "Expand".to_string(),
                is_group: false,
                group_color: None,
            },
            WhichKeyItem {
                key: "e".to_string(),
                label: "e".to_string(),
                description: "Explorer".to_string(),
                is_group: false,
                group_color: None,
            },
            WhichKeyItem {
                key: "<Tab>".to_string(),
                label: "TAB".to_string(),
                description: "spaces".to_string(),
                is_group: true,
                group_color: Some("teal".to_string()),
            },
        ];

        sort_which_key_items(&mut items);
        let keys: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Mappings first: word keys (lowercase before uppercase), then bracketed keys, then bare symbols; then groups.
        assert_eq!(keys, vec!["e", "E", "SPC", "?", "a", "TAB"]);
    }

    #[test]
    fn computes_leader_subgroup_and_all_keymaps_from_vim_profile() {
        let leader = which_key_for_prefix(InputMode::Normal, &["<Space>".to_string()]).expect("leader which-key");
        assert_eq!(leader.title, "SPC");
        assert!(!leader.is_all_keymaps);
        assert!(leader.items.iter().any(|i| i.label == "e" && !i.is_group));
        assert!(leader.items.iter().any(|i| i.label == "a"
            && i.is_group
            && i.description == "ai"
            && i.group_color.as_deref() == Some("pink")));
        // Non-groups must precede groups.
        let first_group = leader.items.iter().position(|i| i.is_group).unwrap();
        let last_mapping = leader.items.iter().rposition(|i| !i.is_group).unwrap();
        assert!(last_mapping < first_group);

        let ai =
            which_key_for_prefix(InputMode::Normal, &["<Space>".to_string(), "a".to_string()]).expect("ai subgroup");
        assert_eq!(ai.title, "SPC » a +ai");
        assert!(
            ai.items
                .iter()
                .any(|i| i.label == "a" && i.description == "Ask Agent About This Tab")
        );
        assert!(
            ai.items
                .iter()
                .any(|i| i.label == "w" && i.description == "Toggle Web Chat")
        );

        let goto = which_key_for_prefix(InputMode::Normal, &["g".to_string()]).expect("goto subgroup");
        assert_eq!(goto.title, "g +goto");
        assert!(
            goto.items
                .iter()
                .any(|i| i.label == "g" && i.description == "Go to Top")
        );

        let all = which_key_all_keymaps(InputMode::Normal);
        assert_eq!(all.title, "Keymaps");
        assert!(all.is_all_keymaps);
        assert!(
            all.items
                .iter()
                .any(|i| i.label == "j" && i.description == "Scroll Down")
        );
        assert!(all.items.iter().any(|i| i.label == "g" && i.is_group));

        // VIM-WK-08: `<C-\>` must sort before `<C-b>`, matching the fork's literal token order.
        let pos_bslash = all.items.iter().position(|i| i.label == "<C-\\>");
        let pos_cb = all.items.iter().position(|i| i.label == "<C-b>");
        assert!(
            pos_bslash.is_some() && pos_cb.is_some() && pos_bslash < pos_cb,
            "expected <C-\\> ({pos_bslash:?}) before <C-b> ({pos_cb:?})"
        );
    }

    #[test]
    fn sample_cycle_covers_every_vim_mode_and_which_key_state() {
        let states: Vec<SampleInputState> = (0..SAMPLE_INPUT_STEP_COUNT).map(sample_input_state).collect();
        for mode in [
            InputMode::Normal,
            InputMode::Insert,
            InputMode::Visual,
            InputMode::Explorer,
            InputMode::Toolbar,
            InputMode::Library,
            InputMode::Window,
        ] {
            assert!(
                states.iter().any(|s| s.mode == mode),
                "sample cycle should include {mode:?}"
            );
        }
        assert!(states.iter().any(|s| s.count == Some(3) && s.pending_keys == vec!["g"]));
        assert!(
            states
                .iter()
                .any(|s| s.which_key.as_ref().is_some_and(|w| w.title == "SPC"))
        );
        assert!(
            states
                .iter()
                .any(|s| s.which_key.as_ref().is_some_and(|w| w.title == "SPC » a +ai"))
        );
        assert!(
            states
                .iter()
                .any(|s| s.which_key.as_ref().is_some_and(|w| w.title == "Keymaps"))
        );
    }
}
