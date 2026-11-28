/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! TOML profile loading and compiled key tables mapping key sequences to command invocations.

use crate::notation::{KeyCode, KeyModifiers, KeySequence, KeyStroke, NotationError};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

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
    Json(String),
    Notation { keys: String, error: NotationError },
    EmptyCommand { keys: String },
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(message) => write!(f, "invalid keymap TOML: {message}"),
            Self::Json(message) => write!(f, "invalid zendia.json: {message}"),
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

/// Metadata extracted from a user's `zendia.json` alongside keymap overrides (`CFG-04`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserConfigMeta {
    pub extras: Vec<String>,
    pub error: Option<String>,
}

fn camel_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for ch in s.chars() {
        if ch.is_ascii_uppercase() {
            out.push('_');
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Translates an action name from the old Zendia fork (e.g. `tabs.next`, `page.hardReload`, `library.history`) into the
/// port's [`CommandInvocation`], leaving native port command names untouched (`CFG-04`, Appendix A).
pub fn translate_fork_action(action: &str) -> CommandInvocation {
    let trimmed = action.trim();
    match trimmed {
        "tabs.next" => CommandInvocation::new("tab.next"),
        "tabs.prev" => CommandInvocation::new("tab.previous"),
        "tabs.moveNext" => CommandInvocation::new("tab.move_next"),
        "tabs.movePrev" => CommandInvocation::new("tab.move_prev"),
        "tabs.alternate" => CommandInvocation::new("tab.alternate"),
        "tabs.close" => CommandInvocation::new("tab.close"),
        "tabs.closeOthers" => CommandInvocation::new("tab.close_others"),
        "tabs.closeRight" => CommandInvocation::new("tab.close_right"),
        "tabs.closeLeft" => CommandInvocation::new("tab.close_left"),
        "tabs.togglePin" => CommandInvocation::new("tab.toggle_pin"),
        "tabs.closeUnpinned" => CommandInvocation::new("tab.close_unpinned"),
        "tabs.pick" => CommandInvocation::new("tab.pick"),
        "tabs.menu" => CommandInvocation::new("tab.menu"),
        "tabs.toggleEssential" => CommandInvocation::new("tab.toggle_essential"),
        "tabs.rename" => CommandInvocation::new("tab.start_rename"),
        "tabs.duplicate" => CommandInvocation::new("tab.duplicate"),
        "tabs.unload" => CommandInvocation::new("tab.unload"),
        "tabs.toggleMute" => CommandInvocation::new("tab.toggle_mute"),
        "tabs.moveToFolder" => CommandInvocation::new("tab.move_to_folder"),
        "tabs.undoClose" => CommandInvocation::new("tab.reopen_closed"),
        "tabs.new" => CommandInvocation::new("address_bar.open").with_arg("new_tab", ArgValue::Bool(true)),
        "history.back" => CommandInvocation::new("navigation.back"),
        "history.forward" => CommandInvocation::new("navigation.forward"),
        "page.reload" => CommandInvocation::new("navigation.reload").with_arg("bypass_cache", ArgValue::Bool(false)),
        "page.hardReload" => CommandInvocation::new("navigation.reload").with_arg("bypass_cache", ArgValue::Bool(true)),
        "yank.url" => CommandInvocation::new("address_bar.copy_current_url"),
        "url.pasteNewTab" => CommandInvocation::new("url.paste_new_tab"),
        "search.page" => CommandInvocation::new("page.find"),
        "find.anything" => CommandInvocation::new("address_bar.open"),
        "spaces.next" => CommandInvocation::new("space.next"),
        "spaces.prev" => CommandInvocation::new("space.previous"),
        "spaces.new" => CommandInvocation::new("space.create_form.open"),
        "spaces.first" => CommandInvocation::new("space.first"),
        "spaces.last" => CommandInvocation::new("space.last"),
        "spaces.close" => CommandInvocation::new("space.close"),
        "spaces.rename" => CommandInvocation::new("space.rename"),
        "spaces.icon" => CommandInvocation::new("space.icon"),
        "spaces.theme" => CommandInvocation::new("space.theme"),
        "spaces.unload" => CommandInvocation::new("space.unload"),
        "panes.focusLeft" | "splits.focusLeft" => CommandInvocation::new("split.focus_left"),
        "panes.focusDown" | "splits.focusDown" => CommandInvocation::new("split.focus_down"),
        "panes.focusUp" | "splits.focusUp" => CommandInvocation::new("split.focus_up"),
        "panes.focusRight" | "splits.focusRight" => CommandInvocation::new("split.focus_right"),
        "splits.next" => CommandInvocation::new("split.next"),
        "splits.last" => CommandInvocation::new("split.last"),
        "splits.pick" => CommandInvocation::new("split.pick"),
        "splits.close" => CommandInvocation::new("split.remove_tab"),
        "splits.only" => CommandInvocation::new("split.unsplit"),
        "splits.zoom" => CommandInvocation::new("split.zoom"),
        "splits.equalize" => CommandInvocation::new("split.equalize"),
        "splits.swap" => CommandInvocation::new("split.swap"),
        "splits.rotate" => CommandInvocation::new("split.rotate"),
        "splits.rotateBack" => CommandInvocation::new("split.rotate_back"),
        "splits.edgeLeft" => CommandInvocation::new("split.edge_left"),
        "splits.edgeDown" => CommandInvocation::new("split.edge_down"),
        "splits.edgeUp" => CommandInvocation::new("split.edge_up"),
        "splits.edgeRight" => CommandInvocation::new("split.edge_right"),
        "splits.growHeight" => CommandInvocation::new("split.grow_height"),
        "splits.shrinkHeight" => CommandInvocation::new("split.shrink_height"),
        "splits.growWidth" => CommandInvocation::new("split.grow_width"),
        "splits.shrinkWidth" => CommandInvocation::new("split.shrink_width"),
        "splits.grab" => CommandInvocation::new("split.grab"),
        "splits.add" => CommandInvocation::new("split.add"),
        "splits.horizontal" => CommandInvocation::new("split.horizontal"),
        "splits.vertical" => CommandInvocation::new("split.vertical"),
        "splits.grid" => {
            CommandInvocation::new("split.set_layout").with_arg("direction", ArgValue::String("grid".to_string()))
        }
        "library.history" => {
            CommandInvocation::new("library.toggle").with_arg("section", ArgValue::String("history".to_string()))
        }
        "library.downloads" => {
            CommandInvocation::new("library.toggle").with_arg("section", ArgValue::String("downloads".to_string()))
        }
        "library.spaces" => {
            CommandInvocation::new("library.toggle").with_arg("section", ArgValue::String("spaces".to_string()))
        }
        "library.media" => {
            CommandInvocation::new("library.toggle").with_arg("section", ArgValue::String("media".to_string()))
        }
        "library.routing" => {
            CommandInvocation::new("library.toggle").with_arg("section", ArgValue::String("routing".to_string()))
        }
        "explorer.add" => CommandInvocation::new("explorer.add"),
        "explorer.addFolder" => CommandInvocation::new("folder.create"),
        "explorer.delete" => CommandInvocation::new("explorer.delete"),
        "explorer.rename" => CommandInvocation::new("tab.start_rename"),
        "explorer.duplicate" => CommandInvocation::new("tab.duplicate"),
        "explorer.togglePin" => CommandInvocation::new("tab.toggle_pin"),
        "explorer.toggleEssential" => CommandInvocation::new("tab.toggle_essential"),
        "explorer.moveToFolder" => CommandInvocation::new("tab.move_to_folder"),
        "explorer.reload" => CommandInvocation::new("navigation.reload"),
        "explorer.unload" => CommandInvocation::new("tab.unload"),
        "explorer.toggleMute" => CommandInvocation::new("tab.toggle_mute"),
        "explorer.escape" => CommandInvocation::new("explorer.escape"),
        "explorer.foldOpenAll" => CommandInvocation::new("folder.expand_all"),
        "explorer.foldCloseAll" => CommandInvocation::new("folder.collapse_all"),
        "toolbar.urlbar" => CommandInvocation::new("toolbar.urlbar"),
        "whichkey.show" => CommandInvocation::new("which_key.show_all"),
        "zen.sidebar.toggle" => CommandInvocation::new("sidebar.toggle_expanded"),
        "zen.compactMode.toggle" => CommandInvocation::new("layout.toggle_compact_mode"),
        "zen.layout.reset" => CommandInvocation::new("layout.reset"),
        "ui.toggleDark" => CommandInvocation::new("theme.toggle_dark"),
        "ui.readerMode" => CommandInvocation::new("site.reader_mode.toggle"),
        "ui.dismissNotifications" => CommandInvocation::new("notification.dismiss_all"),
        "ui.appMenu" => CommandInvocation::new("menu.app.toggle"),
        "session.quit" => CommandInvocation::new("app.quit"),
        "window.toggleMaximize" => CommandInvocation::new("window.toggle_maximize"),
        other => {
            if let Some((ns, rest)) = other.split_once('.')
                && matches!(
                    ns,
                    "scroll"
                        | "edit"
                        | "visual"
                        | "explorer"
                        | "library"
                        | "yank"
                        | "webchat"
                        | "ublock"
                        | "toolbar"
                        | "window"
                )
                && rest.chars().any(|c| c.is_ascii_uppercase())
            {
                CommandInvocation::new(format!("{ns}.{}", camel_to_snake(rest)))
            } else {
                CommandInvocation::new(other)
            }
        }
    }
}

/// A parsed keyboard profile before compilation into a lookup trie.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub description: Option<String>,
    pub leader: KeyStroke,
    pub leader_sequence: KeySequence,
    pub bindings: Vec<Binding>,
    pub groups: Vec<GroupSpec>,
    unmapped: BTreeSet<(Option<String>, KeySequence)>,
}

impl Profile {
    /// Parses a keyboard profile from TOML source.
    pub fn from_toml(source: &str) -> Result<Self, TableError> {
        Self::from_toml_with_leader(source, None)
    }

    /// Parses a keyboard profile from TOML source with an optional `leader` override (`CFG-05`).
    pub fn from_toml_with_leader(source: &str, leader_override: Option<&str>) -> Result<Self, TableError> {
        let raw: RawProfile = toml::from_str(source).map_err(|error| TableError::Toml(error.to_string()))?;
        let leader_text = leader_override.filter(|s| !s.is_empty()).or(raw.leader.as_deref());
        let leader_sequence = match leader_text {
            Some(text) => {
                if text.to_ascii_lowercase().contains("<leader>") {
                    return Err(TableError::Notation {
                        keys: text.to_string(),
                        error: NotationError::UnknownKey("<leader>".to_string()),
                    });
                }
                KeySequence::parse(text).map_err(|error| TableError::Notation {
                    keys: text.to_string(),
                    error,
                })?
            }
            None => KeySequence::from_strokes(vec![KeyStroke::new(KeyModifiers::NONE, KeyCode::Space)]).unwrap(),
        };
        let leader = leader_sequence.strokes()[0];

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
                    keys: parsed.expand_leader_strokes(leader_sequence.strokes()),
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
                keys: parsed.expand_leader_strokes(leader_sequence.strokes()),
                name: entry.name,
                color: entry.color,
                mode: entry.mode,
            });
        }

        Ok(Self {
            name: raw.name,
            description: raw.description,
            leader,
            leader_sequence,
            bindings,
            groups,
            unmapped: BTreeSet::new(),
        })
    }

    /// Loads the embedded standard browser keyboard profile.
    pub fn standard() -> Self {
        Self::from_toml(STANDARD_PROFILE_TOML).expect("embedded standard.toml must be valid")
    }

    /// Loads the embedded default vim keyboard profile, with the leader keys of normal mode also reaching the
    /// browser's own surfaces ([`CHROME_MODES`]).
    pub fn vim() -> Self {
        Self::vim_configured(None, None).expect("embedded vim.toml must be valid")
    }

    /// Builds a configured vim [`Profile`] with an optional custom `leader` (`CFG-05`) and optional user
    /// `zendia.json` source (`CFG-04`), applying user overrides at priority 1000 before inheriting normal-mode
    /// `<leader>` keys into [`CHROME_MODES`].
    pub fn vim_configured(leader_override: Option<&str>, user_json: Option<&str>) -> Result<Self, TableError> {
        let mut profile = Self::from_toml_with_leader(VIM_PROFILE_TOML, leader_override)?;
        if let Some(json) = user_json
            && !json.trim().is_empty()
        {
            profile.apply_user_json(json)?;
        }
        profile.inherit_leader_keys("n", &CHROME_MODES);
        Ok(profile)
    }

    /// Resilient variant of [`Self::vim_configured`] that falls back to the base profile and records any parse error
    /// inside [`UserConfigMeta::error`] (`CFG-04`, `CFG-05`).
    pub fn vim_configured_with_meta(leader_override: Option<&str>, user_json: Option<&str>) -> (Self, UserConfigMeta) {
        let mut meta = UserConfigMeta::default();
        let mut profile = match Self::from_toml_with_leader(VIM_PROFILE_TOML, leader_override) {
            Ok(p) => p,
            Err(err) => {
                meta.error = Some(err.to_string());
                Self::from_toml_with_leader(VIM_PROFILE_TOML, None).expect("embedded vim.toml must be valid")
            }
        };
        if let Some(json) = user_json
            && !json.trim().is_empty()
        {
            match profile.apply_user_json(json) {
                Ok(parsed_meta) => {
                    meta.extras = parsed_meta.extras;
                }
                Err(err) => {
                    meta.error = Some(err.to_string());
                }
            }
        }
        profile.inherit_leader_keys("n", &CHROME_MODES);
        (profile, meta)
    }

    /// Applies a user `zendia.json` document (`extras`, `plugins`, and priority-1000 `keymaps`, where `rhs: false`
    /// removes an existing mapping, `CFG-04`).
    pub fn apply_user_json(&mut self, json_source: &str) -> Result<UserConfigMeta, TableError> {
        let root: serde_json::Value =
            serde_json::from_str(json_source).map_err(|err| TableError::Json(err.to_string()))?;
        let Some(obj) = root.as_object() else {
            return Err(TableError::Json("zendia.json must hold a JSON object".to_string()));
        };

        let mut meta = UserConfigMeta::default();
        if let Some(extras_val) = obj.get("extras") {
            let Some(arr) = extras_val.as_array() else {
                return Err(TableError::Json("\"extras\" must be a list of names".to_string()));
            };
            for item in arr {
                let Some(name) = item.as_str() else {
                    return Err(TableError::Json("\"extras\" must be a list of names".to_string()));
                };
                meta.extras.push(name.to_string());
            }
        }

        let mut specs_with_priority: Vec<(i64, usize, &serde_json::Value)> = Vec::new();
        if let Some(plugins_val) = obj.get("plugins") {
            let Some(arr) = plugins_val.as_array() else {
                return Err(TableError::Json("\"plugins\" must be a list of specs".to_string()));
            };
            for (idx, plugin) in arr.iter().enumerate() {
                let Some(p_obj) = plugin.as_object() else {
                    return Err(TableError::Json("each plugin spec must be an object".to_string()));
                };
                if p_obj.get("enabled").and_then(serde_json::Value::as_bool) == Some(false) {
                    continue;
                }
                let prio = p_obj.get("priority").and_then(serde_json::Value::as_i64).unwrap_or(50);
                specs_with_priority.push((prio, idx, plugin));
            }
        }
        specs_with_priority.sort_by_key(|(prio, idx, _)| (*prio, *idx));
        for (_, _, plugin) in specs_with_priority {
            if let Some(groups) = plugin.get("groups").and_then(serde_json::Value::as_array) {
                for entry in groups {
                    self.apply_user_group_entry(entry)?;
                }
            }
            if let Some(keys) = plugin.get("keys").and_then(serde_json::Value::as_array) {
                for entry in keys {
                    self.apply_user_key_entry(entry)?;
                }
            }
        }

        if let Some(keymaps_val) = obj.get("keymaps") {
            let Some(arr) = keymaps_val.as_array() else {
                return Err(TableError::Json("\"keymaps\" must be a list of keys".to_string()));
            };
            for entry in arr {
                self.apply_user_key_entry(entry)?;
            }
        }

        Ok(meta)
    }

    fn apply_user_group_entry(&mut self, entry: &serde_json::Value) -> Result<(), TableError> {
        let (modes_val, lhs, name, color) = if let Some(arr) = entry.as_array() {
            if arr.len() < 3 {
                return Err(TableError::Json(
                    "group entry array needs [mode, lhs, name]".to_string(),
                ));
            }
            let Some(lhs) = arr[1].as_str() else {
                return Err(TableError::Json("group lhs must be a string".to_string()));
            };
            let Some(name) = arr[2].as_str() else {
                return Err(TableError::Json("group name must be a string".to_string()));
            };
            (&arr[0], lhs, name, None)
        } else if let Some(obj) = entry.as_object() {
            let Some(lhs) = obj.get("lhs").and_then(serde_json::Value::as_str) else {
                return Err(TableError::Json("group object missing string `lhs`".to_string()));
            };
            let Some(name) = obj.get("name").and_then(serde_json::Value::as_str) else {
                return Err(TableError::Json("group object missing string `name`".to_string()));
            };
            let mode_v = obj.get("mode").unwrap_or(&serde_json::Value::Null);
            let color = obj.get("color").and_then(serde_json::Value::as_str).map(str::to_string);
            (mode_v, lhs, name, color)
        } else {
            return Err(TableError::Json(
                "group entry must be an array or an object".to_string(),
            ));
        };

        let parsed = KeySequence::parse(lhs).map_err(|error| TableError::Notation {
            keys: lhs.to_string(),
            error,
        })?;
        let expanded = parsed.expand_leader_strokes(self.leader_sequence.strokes());
        for mode in parse_user_modes(modes_val)? {
            self.groups.push(GroupSpec {
                keys: expanded.clone(),
                name: name.to_string(),
                color: color.clone(),
                mode,
            });
        }
        Ok(())
    }

    fn apply_user_key_entry(&mut self, entry: &serde_json::Value) -> Result<(), TableError> {
        let (modes_val, lhs, rhs, opts_obj) = if let Some(arr) = entry.as_array() {
            if arr.len() < 3 {
                return Err(TableError::Json("key entry array needs [mode, lhs, rhs]".to_string()));
            }
            let Some(lhs) = arr[1].as_str() else {
                return Err(TableError::Json("key entry `lhs` must be a string".to_string()));
            };
            let opts = arr.get(3).and_then(serde_json::Value::as_object);
            (&arr[0], lhs, &arr[2], opts)
        } else if let Some(obj) = entry.as_object() {
            let Some(lhs) = obj.get("lhs").and_then(serde_json::Value::as_str) else {
                return Err(TableError::Json("key entry object missing string `lhs`".to_string()));
            };
            let Some(rhs) = obj.get("rhs") else {
                return Err(TableError::Json("key entry object missing `rhs`".to_string()));
            };
            let mode_v = obj.get("mode").unwrap_or(&serde_json::Value::Null);
            (mode_v, lhs, rhs, Some(obj))
        } else {
            return Err(TableError::Json("key entry must be an array or an object".to_string()));
        };

        let parsed = KeySequence::parse(lhs).map_err(|error| TableError::Notation {
            keys: lhs.to_string(),
            error,
        })?;
        let expanded = parsed.expand_leader_strokes(self.leader_sequence.strokes());
        let modes = parse_user_modes(modes_val)?;

        if rhs.as_bool() == Some(false) {
            for mode in modes {
                self.bindings.retain(|b| !(b.mode == mode && b.keys == expanded));
                self.unmapped.insert((mode, expanded.clone()));
            }
            return Ok(());
        }

        let Some(action_str) = rhs.as_str() else {
            return Err(TableError::Json(
                "key entry `rhs` must be an action name string or `false`".to_string(),
            ));
        };
        if action_str.trim().is_empty() {
            return Err(TableError::EmptyCommand { keys: lhs.to_string() });
        }

        let mut invocation = translate_fork_action(action_str);
        let mut desc = None;
        if let Some(opts) = opts_obj {
            if let Some(d) = opts
                .get("desc")
                .or_else(|| opts.get("description"))
                .and_then(serde_json::Value::as_str)
            {
                desc = Some(d.to_string());
            }
            if let Some(args_map) = opts.get("args").and_then(serde_json::Value::as_object) {
                for (k, v) in args_map {
                    if let Some(b) = v.as_bool() {
                        invocation.args.insert(k.clone(), ArgValue::Bool(b));
                    } else if let Some(n) = v.as_i64() {
                        invocation.args.insert(k.clone(), ArgValue::Int(n));
                    } else if let Some(s) = v.as_str() {
                        invocation.args.insert(k.clone(), ArgValue::String(s.to_string()));
                    }
                }
            }
        }

        for mode in modes {
            self.unmapped.remove(&(mode.clone(), expanded.clone()));
            self.bindings.push(Binding {
                keys: expanded.clone(),
                invocation: invocation.clone(),
                description: desc.clone(),
                mode,
            });
        }
        Ok(())
    }

    /// Copies every binding and group of mode `from` that starts with the leader into each mode of `to` that does not
    /// map (or explicitly unmap) the same keys itself, so `<leader>` works the same while the explorer, the toolbar,
    /// the library or window mode is open.
    pub fn inherit_leader_keys(&mut self, from: &str, to: &[&str]) {
        let leader_strokes = self.leader_sequence.strokes();
        let starts_with_leader =
            |keys: &KeySequence| !leader_strokes.is_empty() && keys.strokes().starts_with(leader_strokes);
        for &mode in to {
            let mode_key = Some(mode.to_string());
            let own_bindings: BTreeSet<KeySequence> = self
                .bindings
                .iter()
                .filter(|binding| binding.mode.as_deref() == Some(mode))
                .map(|binding| binding.keys.clone())
                .collect();
            let inherited: Vec<Binding> = self
                .bindings
                .iter()
                .filter(|binding| {
                    binding.mode.as_deref() == Some(from)
                        && starts_with_leader(&binding.keys)
                        && !own_bindings.contains(&binding.keys)
                        && !self.unmapped.contains(&(mode_key.clone(), binding.keys.clone()))
                })
                .map(|binding| Binding {
                    mode: mode_key.clone(),
                    ..binding.clone()
                })
                .collect();
            self.bindings.extend(inherited);

            let own_groups: BTreeSet<KeySequence> = self
                .groups
                .iter()
                .filter(|group| group.mode.as_deref() == Some(mode))
                .map(|group| group.keys.clone())
                .collect();
            let inherited: Vec<GroupSpec> = self
                .groups
                .iter()
                .filter(|group| {
                    group.mode.as_deref() == Some(from)
                        && starts_with_leader(&group.keys)
                        && !own_groups.contains(&group.keys)
                })
                .map(|group| GroupSpec {
                    mode: mode_key.clone(),
                    ..group.clone()
                })
                .collect();
            self.groups.extend(inherited);
        }
    }
}

fn parse_user_modes(value: &serde_json::Value) -> Result<Vec<Option<String>>, TableError> {
    match value {
        serde_json::Value::Null => Ok(vec![Some("n".to_string())]),
        serde_json::Value::String(s) => {
            if s.is_empty() {
                return Err(TableError::Json("key entry `mode` cannot be empty".to_string()));
            }
            let mapped = if s == "x" { "v" } else { s.as_str() };
            Ok(vec![Some(mapped.to_string())])
        }
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                return Err(TableError::Json("key entry `mode` list cannot be empty".to_string()));
            }
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let Some(s) = item.as_str() else {
                    return Err(TableError::Json("key entry `mode` items must be strings".to_string()));
                };
                let mapped = if s == "x" { "v" } else { s };
                out.push(Some(mapped.to_string()));
            }
            Ok(out)
        }
        _ => Err(TableError::Json(
            "key entry `mode` must be a string or list of strings".to_string(),
        )),
    }
}

/// The vim scheme's modes for the browser's own surfaces: the explorer (`e`), the toolbar (`t`), the library (`l`)
/// and window mode (`w`). They take counts and leave unmapped keys alone, and they inherit normal mode's leader keys.
pub const CHROME_MODES: [&str; 4] = ["e", "t", "l", "w"];

#[derive(Clone, Debug, Default)]
pub(crate) struct TrieNode {
    pub(crate) binding: Option<Binding>,
    pub(crate) children: BTreeMap<KeyStroke, TrieNode>,
}

impl TrieNode {
    /// A key sequence can be a mapping and a prefix of longer ones at once (`g` and `gg`), as in vim. The key engine
    /// settles such a sequence with `timeoutlen`. A later binding of the same sequence replaces the earlier one.
    fn insert(&mut self, strokes: &[KeyStroke], binding: Binding) {
        let Some((first, rest)) = strokes.split_first() else {
            self.binding = Some(binding);
            return;
        };
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
    fn rejects_empty_command_or_empty_keys() {
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
                keys = ""
                command = "tab.new"
                "#
            ),
            Err(TableError::Notation { .. })
        ));
    }

    #[test]
    fn applies_user_json_keymaps_and_custom_leader() {
        let user_json = r#"{
            "extras": ["ublock"],
            "keymaps": [
                ["n", "<C-n>", "tabs.next", { "desc": "Next Tab" }],
                ["n", "J", false],
                ["n", "<leader>uz", "ublock.zapper", { "desc": "Element Zapper" }],
                ["e", "<leader>bd", false]
            ]
        }"#;
        let profile = Profile::vim_configured(Some(","), Some(user_json)).unwrap();
        assert_eq!(profile.leader, KeyStroke::char(','));

        let normal = KeyTable::from_profile_for_mode(&profile, "n");
        let ctrl_n = KeySequence::parse("<C-n>").unwrap();
        assert_eq!(
            normal
                .node_at(ctrl_n.strokes())
                .and_then(|n| n.binding.as_ref())
                .map(|b| b.invocation.command.as_str()),
            Some("tab.next")
        );
        // `J` was unmapped with `false`.
        assert!(
            normal
                .node_at(KeySequence::parse("J").unwrap().strokes())
                .and_then(|n| n.binding.as_ref())
                .is_none()
        );
        // `<leader>uz` expanded with custom `,` leader and inherited into explorer mode (`e`).
        let leader_uz = KeySequence::parse(",uz").unwrap();
        assert_eq!(
            normal
                .node_at(leader_uz.strokes())
                .and_then(|n| n.binding.as_ref())
                .map(|b| b.invocation.command.as_str()),
            Some("ublock.zapper")
        );
        let explorer = KeyTable::from_profile_for_mode(&profile, "e");
        assert_eq!(
            explorer
                .node_at(leader_uz.strokes())
                .and_then(|n| n.binding.as_ref())
                .map(|b| b.invocation.command.as_str()),
            Some("ublock.zapper")
        );
        // `,bd` was explicitly unmapped in `"e"`, so `inherit_leader_keys` does not re-add it.
        assert!(
            explorer
                .node_at(KeySequence::parse(",bd").unwrap().strokes())
                .and_then(|n| n.binding.as_ref())
                .is_none()
        );
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
