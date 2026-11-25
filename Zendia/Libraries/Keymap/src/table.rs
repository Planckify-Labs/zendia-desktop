/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! TOML profile loading and compiled key tables mapping key sequences to command invocations.

use crate::notation::{KeyCode, KeyModifiers, KeySequence, KeyStroke, NotationError};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

/// Embedded default standard browser keyboard profile (`Zendia/Base/res/keyboard/standard.toml`).
pub const STANDARD_PROFILE_TOML: &str = include_str!("../../../Base/res/keyboard/standard.toml");
/// Embedded default vim keyboard profile (`Zendia/Base/res/keyboard/vim.toml`).
pub const VIM_PROFILE_TOML: &str = include_str!("../../../Base/res/keyboard/vim.toml");

/// Typed scalar value passed as a command argument in a key binding.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    Bool(bool),
    Int(i64),
    String(String),
}

impl ArgValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value.as_str()),
            _ => None,
        }
    }

    /// Formats the argument value as a string for generic command dispatchers.
    pub fn to_arg_string(&self) -> String {
        match self {
            Self::Bool(value) => value.to_string(),
            Self::Int(value) => value.to_string(),
            Self::String(value) => value.clone(),
        }
    }
}

/// Command name and arguments invoked when a binding matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandInvocation {
    pub command: String,
    pub args: BTreeMap<String, ArgValue>,
}

impl CommandInvocation {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: BTreeMap::new(),
        }
    }

    pub fn with_arg(mut self, name: impl Into<String>, value: ArgValue) -> Self {
        self.args.insert(name.into(), value);
        self
    }

    pub fn args_as_pairs(&self) -> Vec<(String, String)> {
        self.args
            .iter()
            .map(|(key, value)| (key.clone(), value.to_arg_string()))
            .collect()
    }
}

/// One resolved entry in a profile or compiled key table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub keys: KeySequence,
    pub invocation: CommandInvocation,
    pub description: Option<String>,
    pub mode: Option<String>,
}

/// A named which-key group associated with a key prefix (for example `<leader>a` -> `ai`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupSpec {
    pub keys: KeySequence,
    pub name: String,
    pub color: Option<String>,
    pub mode: Option<String>,
}

/// Direct child of a prefix node inside a compiled [`KeyTable`], used to build which-key views.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrieChild {
    pub stroke: KeyStroke,
    pub binding: Option<Binding>,
    pub is_group: bool,
    pub descendant_count: usize,
}

/// Errors encountered while loading or compiling a keyboard profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableError {
    Toml(String),
    Notation { keys: String, error: NotationError },
    EmptyCommand { keys: String },
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(message) => write!(f, "invalid keymap TOML: {message}"),
            Self::Notation { keys, error } => write!(f, "invalid key notation `{keys}`: {error}"),
            Self::EmptyCommand { keys } => write!(f, "binding `{keys}` has an empty command name"),
        }
    }
}

impl std::error::Error for TableError {}

#[derive(Deserialize)]
#[serde(untagged)]
enum KeysField {
    One(String),
    Many(Vec<String>),
}

impl KeysField {
    fn into_vec(self) -> Vec<String> {
        match self {
            Self::One(value) => vec![value],
            Self::Many(values) => values,
        }
    }
}

#[derive(Deserialize)]
struct RawBinding {
    keys: KeysField,
    command: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    args: BTreeMap<String, ArgValue>,
}

#[derive(Deserialize)]
struct RawGroup {
    keys: String,
    name: String,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Deserialize)]
struct RawProfile {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    leader: Option<String>,
    #[serde(default)]
    bindings: Vec<RawBinding>,
    #[serde(default)]
    groups: Vec<RawGroup>,
}

/// A parsed keyboard profile before compilation into a lookup trie.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub description: Option<String>,
    pub leader: KeyStroke,
    pub bindings: Vec<Binding>,
    pub groups: Vec<GroupSpec>,
}

impl Profile {
    /// Parses a keyboard profile from TOML source.
    pub fn from_toml(source: &str) -> Result<Self, TableError> {
        let raw: RawProfile = toml::from_str(source).map_err(|error| TableError::Toml(error.to_string()))?;
        let leader = match raw.leader {
            Some(ref text) => KeyStroke::from_str(text).map_err(|error| TableError::Notation {
                keys: text.clone(),
                error,
            })?,
            None => KeyStroke::new(KeyModifiers::NONE, KeyCode::Space),
        };

        let mut bindings = Vec::new();
        for entry in raw.bindings {
            let command = entry.command.trim().to_string();
            for raw_keys in entry.keys.into_vec() {
                if command.is_empty() {
                    return Err(TableError::EmptyCommand { keys: raw_keys });
                }
                let parsed =
                    KeySequence::parse(&raw_keys).map_err(|error| TableError::Notation { keys: raw_keys, error })?;
                bindings.push(Binding {
                    keys: parsed.expand_leader(leader),
                    invocation: CommandInvocation {
                        command: command.clone(),
                        args: entry.args.clone(),
                    },
                    description: entry.description.clone(),
                    mode: entry.mode.clone(),
                });
            }
        }

        let mut groups = Vec::new();
        for entry in raw.groups {
            let parsed = KeySequence::parse(&entry.keys).map_err(|error| TableError::Notation {
                keys: entry.keys,
                error,
            })?;
            groups.push(GroupSpec {
                keys: parsed.expand_leader(leader),
                name: entry.name,
                color: entry.color,
                mode: entry.mode,
            });
        }

        Ok(Self {
            name: raw.name,
            description: raw.description,
            leader,
            bindings,
            groups,
        })
    }

    /// Loads the embedded standard browser keyboard profile.
    pub fn standard() -> Self {
        Self::from_toml(STANDARD_PROFILE_TOML).expect("embedded standard.toml must be valid")
    }

    /// Loads the embedded default vim keyboard profile.
    pub fn vim() -> Self {
        Self::from_toml(VIM_PROFILE_TOML).expect("embedded vim.toml must be valid")
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TrieNode {
    pub(crate) binding: Option<Binding>,
    pub(crate) children: BTreeMap<KeyStroke, TrieNode>,
}

impl TrieNode {
    fn insert(&mut self, strokes: &[KeyStroke], binding: Binding) {
        let Some((first, rest)) = strokes.split_first() else {
            // Installing a leaf at this prefix replaces any longer conflicting descendants.
            self.children.clear();
            self.binding = Some(binding);
            return;
        };
        // Extending past a previously bound prefix replaces the shorter binding so resolution stays prefix-free.
        self.binding = None;
        self.children.entry(*first).or_default().insert(rest, binding);
    }

    pub(crate) fn get(&self, strokes: &[KeyStroke]) -> Option<&TrieNode> {
        let mut current = self;
        for stroke in strokes {
            current = current.children.get(stroke)?;
        }
        Some(current)
    }

    fn collect_bindings<'a>(&'a self, out: &mut Vec<&'a Binding>) {
        if let Some(binding) = &self.binding {
            out.push(binding);
        }
        for child in self.children.values() {
            child.collect_bindings(out);
        }
    }

    fn count_bindings(&self) -> usize {
        let own = usize::from(self.binding.is_some());
        own + self.children.values().map(TrieNode::count_bindings).sum::<usize>()
    }
}

/// Compiled, prefix-free lookup table built from one or more layered keyboard profiles.
#[derive(Clone, Debug, Default)]
pub struct KeyTable {
    root: TrieNode,
    bindings: Vec<Binding>,
    groups: BTreeMap<Vec<KeyStroke>, GroupSpec>,
}

impl KeyTable {
    /// Compiles a single profile into a key table.
    pub fn from_profile(profile: &Profile) -> Self {
        Self::from_profiles(std::slice::from_ref(profile))
    }

    /// Compiles a stack of layered profiles into a key table (later profiles override earlier ones).
    pub fn from_profiles(profiles: &[Profile]) -> Self {
        Self::from_profiles_filtered(profiles, None)
    }

    /// Compiles a single profile filtered to bindings and groups for `mode` (or unscoped entries).
    pub fn from_profile_for_mode(profile: &Profile, mode: &str) -> Self {
        Self::from_profiles_filtered(std::slice::from_ref(profile), Some(mode))
    }

    fn from_profiles_filtered(profiles: &[Profile], mode: Option<&str>) -> Self {
        let mut root = TrieNode::default();
        let mut groups = BTreeMap::new();
        for profile in profiles {
            for binding in &profile.bindings {
                if let Some(target_mode) = mode
                    && let Some(binding_mode) = binding.mode.as_deref()
                    && binding_mode != target_mode
                {
                    continue;
                }
                root.insert(binding.keys.strokes(), binding.clone());
            }
            for group in &profile.groups {
                if let Some(target_mode) = mode
                    && let Some(group_mode) = group.mode.as_deref()
                    && group_mode != target_mode
                {
                    continue;
                }
                groups.insert(group.keys.strokes().to_vec(), group.clone());
            }
        }
        let mut flat = Vec::new();
        root.collect_bindings(&mut flat);
        let bindings = flat.into_iter().cloned().collect();
        Self { root, bindings, groups }
    }

    /// Parses and compiles a TOML keyboard profile directly into a key table.
    pub fn from_toml(source: &str) -> Result<Self, TableError> {
        let profile = Profile::from_toml(source)?;
        Ok(Self::from_profile(&profile))
    }

    /// Returns the compiled standard browser keyboard table.
    pub fn standard() -> Self {
        Self::from_profile(&Profile::standard())
    }

    /// Returns the compiled vim keyboard table for `mode` (`"n"`, `"i"`, `"v"`, `"e"`, `"t"`, `"l"`, `"w"`).
    pub fn vim_for_mode(mode: &str) -> Self {
        Self::from_profile_for_mode(&Profile::vim(), mode)
    }

    /// Returns all active bindings in deterministic key order.
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// Returns all configured which-key groups in deterministic key order.
    pub fn groups(&self) -> impl Iterator<Item = &GroupSpec> {
        self.groups.values()
    }

    /// Returns the configured which-key group metadata for a prefix sequence, if any.
    pub fn group_for(&self, prefix: &[KeyStroke]) -> Option<&GroupSpec> {
        self.groups.get(prefix)
    }

    /// Returns the direct child strokes available under `prefix` (empty for root-level bindings).
    pub fn children_at(&self, prefix: &[KeyStroke]) -> Vec<TrieChild> {
        let Some(node) = self.node_at(prefix) else {
            return Vec::new();
        };
        node.children
            .iter()
            .map(|(&stroke, child)| TrieChild {
                stroke,
                binding: child.binding.clone(),
                is_group: !child.children.is_empty(),
                descendant_count: child.count_bindings(),
            })
            .collect()
    }

    /// Returns the first key sequence bound to `command`, if any (useful for menus and the palette).
    pub fn shortcut_for_command(&self, command: &str) -> Option<&KeySequence> {
        self.bindings
            .iter()
            .find(|binding| binding.invocation.command == command)
            .map(|binding| &binding.keys)
    }

    pub(crate) fn node_at(&self, strokes: &[KeyStroke]) -> Option<&TrieNode> {
        self.root.get(strokes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_embedded_standard_profile() {
        let table = KeyTable::standard();
        assert!(!table.bindings().is_empty());
        assert!(table.shortcut_for_command("address_bar.open").is_some());
        assert!(table.shortcut_for_command("palette.open").is_some());
        assert!(table.shortcut_for_command("tab.close").is_some());
        assert!(table.shortcut_for_command("tab.next").is_some());
        assert!(table.shortcut_for_command("tab.previous").is_some());
        assert!(table.shortcut_for_command("location.focus").is_some());
        assert!(table.shortcut_for_command("navigation.reload").is_some());
        assert!(table.shortcut_for_command("navigation.back").is_some());
        assert!(table.shortcut_for_command("navigation.forward").is_some());
        assert!(table.shortcut_for_command("debug.cycle_input_state").is_some());
    }

    #[test]
    fn layers_user_profile_over_base_profile() {
        let base = Profile::from_toml(
            r#"
            name = "base"
            [[bindings]]
            keys = "<C-t>"
            command = "tab.new"
            [[bindings]]
            keys = "<C-w>"
            command = "tab.close"
            "#,
        )
        .unwrap();
        let override_profile = Profile::from_toml(
            r#"
            name = "user"
            [[bindings]]
            keys = "<C-t>"
            command = "custom.new_tab"
            args = { url = "https://example.com" }
            "#,
        )
        .unwrap();

        let table = KeyTable::from_profiles(&[base, override_profile]);
        let ct = KeySequence::parse("<C-t>").unwrap();
        let node = table.node_at(ct.strokes()).unwrap();
        let binding = node.binding.as_ref().unwrap();
        assert_eq!(binding.invocation.command, "custom.new_tab");
        assert_eq!(
            binding.invocation.args.get("url"),
            Some(&ArgValue::String("https://example.com".to_string()))
        );
        assert!(table.shortcut_for_command("tab.close").is_some());
    }

    #[test]
    fn rejects_empty_command_or_bad_keys() {
        assert!(matches!(
            Profile::from_toml(
                r#"
                name = "bad"
                [[bindings]]
                keys = "<C-t>"
                command = "   "
                "#
            ),
            Err(TableError::EmptyCommand { .. })
        ));
        assert!(matches!(
            Profile::from_toml(
                r#"
                name = "bad"
                [[bindings]]
                keys = "<BogusKey>"
                command = "tab.new"
                "#
            ),
            Err(TableError::Notation { .. })
        ));
    }

    #[test]
    fn loads_embedded_vim_profile_with_modes_and_groups() {
        let profile = Profile::vim();
        assert_eq!(profile.name, "vim");
        assert!(!profile.bindings.is_empty());
        assert!(!profile.groups.is_empty());

        let normal = KeyTable::vim_for_mode("n");
        let leader_a = KeySequence::parse("<Space>a").unwrap();
        let ai_group = normal.group_for(leader_a.strokes()).expect("normal <Space>a group");
        assert_eq!(ai_group.name, "ai");
        assert_eq!(ai_group.color.as_deref(), Some("pink"));

        let leader_children = normal.children_at(KeySequence::parse("<Space>").unwrap().strokes());
        assert!(
            leader_children
                .iter()
                .any(|c| c.stroke == KeyStroke::char('a') && c.is_group)
        );
        assert!(
            leader_children
                .iter()
                .any(|c| c.stroke == KeyStroke::char('?') && !c.is_group)
        );

        let ai_children = normal.children_at(leader_a.strokes());
        assert!(ai_children.len() >= 10);

        // Mode filtering isolates explorer bindings from normal mode.
        let explorer = KeyTable::vim_for_mode("e");
        let fold_group = explorer.group_for(KeySequence::parse("z").unwrap().strokes()).unwrap();
        assert_eq!(fold_group.name, "fold");
        assert!(normal.group_for(KeySequence::parse("z").unwrap().strokes()).is_none());
    }
}
