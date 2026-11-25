/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Space management helpers and URL space routing rules.
//!
//! Routing rules live in profile settings on [`crate::model::BrowserModel`] and send matching URLs to a chosen space
//! (or back to the most recent space). Matching is pure and deterministic: rules are tested in order, the first match
//! wins, and an invalid regular expression never matches.

use crate::commands::{ArgumentSchema, ArgumentType, Command, CommandDescriptor, CommandError};
use crate::menus::ResolvedMenuItem;
use crate::model::{BrowserModel, Space, SpaceGradient, SpaceId, TabId};
use regex::RegexBuilder;
use zendia_keymap::KeyTable;

/// Stable identifier for a URL space routing rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpaceRouteId(pub u64);

/// How a [`SpaceRouteRule`] compares its pattern against a URL.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RouteMatchType {
    /// Matches when the URL contains the pattern (case-insensitive).
    #[default]
    Contains,
    /// Matches when normalized URLs (without scheme, `www.`, or trailing `/`) are equal.
    EqualTo,
    /// Matches when the case-insensitive regular expression matches the URL.
    Regex,
}

impl RouteMatchType {
    /// All match types in the order the routing form displays them.
    pub const ALL: [RouteMatchType; 3] = [RouteMatchType::Contains, RouteMatchType::EqualTo, RouteMatchType::Regex];

    /// Command and serialization identifier matching Zen's routing format.
    pub fn id(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::EqualTo => "equal-to",
            Self::Regex => "regex",
        }
    }

    /// Human-readable chip and row label (matching Zen's locale strings).
    pub fn label(self) -> &'static str {
        match self {
            Self::Contains => "Contains",
            Self::EqualTo => "Is Exactly",
            Self::Regex => "RegEx",
        }
    }

    /// Placeholder text shown in the pattern field for this match type.
    pub fn placeholder(self) -> &'static str {
        match self {
            Self::Regex => r"zendia\.app",
            Self::Contains | Self::EqualTo => "zendia.app",
        }
    }

    /// Parses a match type from a command argument string.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "contains" => Some(Self::Contains),
            "equal-to" | "equal_to" | "equals" | "exact" => Some(Self::EqualTo),
            "regex" | "regexp" => Some(Self::Regex),
            _ => None,
        }
    }
}

/// Destination space for a routing rule or the default external-link policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RouteTarget {
    /// Leave the link in whichever space the user was most recently using.
    #[default]
    MostRecentSpace,
    /// Send the link to a specific workspace.
    Space(SpaceId),
}

impl RouteTarget {
    /// Encodes the target as a command argument string (`most-recent-space` or numeric space id).
    pub fn to_arg_string(self) -> String {
        match self {
            Self::MostRecentSpace => "most-recent-space".to_string(),
            Self::Space(id) => id.0.to_string(),
        }
    }

    /// Parses a target from a command argument string.
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.eq_ignore_ascii_case("most-recent-space") || trimmed.eq_ignore_ascii_case("most_recent_space") {
            return Some(Self::MostRecentSpace);
        }
        let id_str = trimmed.strip_prefix("space:").unwrap_or(trimmed);
        id_str.parse::<u64>().ok().map(|id| Self::Space(SpaceId(id)))
    }
}

/// One URL routing rule sending matching links to a target space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceRouteRule {
    pub id: SpaceRouteId,
    pub match_type: RouteMatchType,
    pub pattern: String,
    pub target: RouteTarget,
}

/// Profile settings for URL space routing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpaceRoutingSettings {
    pub rules: Vec<SpaceRouteRule>,
    pub default_external: RouteTarget,
}

/// Curated selectable space icon names supported by the built-in icon table.
pub const SELECTABLE_SPACE_ICONS: &[&str] = &[
    "briefcase",
    "book",
    "code",
    "compass",
    "flame",
    "heart",
    "star",
    "rocket",
    "terminal",
    "music",
    "cloud",
    "bell",
    "key",
    "lock",
    "eye",
    "people",
    "airplane",
    "mail",
    "github",
    "rss",
    "chat",
    "layers",
];

/// Validates a rule's pattern for the given [`RouteMatchType`].
pub fn validate_pattern(match_type: RouteMatchType, pattern: &str) -> Result<(), String> {
    let trimmed = pattern.trim();
    if trimmed.is_empty() {
        return Err("Pattern cannot be empty".to_string());
    }
    if match_type == RouteMatchType::Regex {
        RegexBuilder::new(trimmed)
            .case_insensitive(true)
            .build()
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

/// Returns `true` when `pattern` is non-empty and compiles cleanly for `match_type`.
pub fn is_valid_pattern(match_type: RouteMatchType, pattern: &str) -> bool {
    validate_pattern(match_type, pattern).is_ok()
}

/// Normalizes a URL for `EqualTo` comparison: trims, lowercases, removes `http(s)://`, `www.`, and a trailing `/`.
pub fn normalize_url_for_equality(url: &str) -> String {
    let mut clean = url.trim().to_ascii_lowercase();
    if let Some(rest) = clean.strip_prefix("https://") {
        clean = rest.to_string();
    } else if let Some(rest) = clean.strip_prefix("http://") {
        clean = rest.to_string();
    }
    if let Some(rest) = clean.strip_prefix("www.") {
        clean = rest.to_string();
    }
    if let Some(rest) = clean.strip_suffix('/') {
        clean = rest.to_string();
    }
    clean
}

/// Evaluates whether `rule` matches `url`. Invalid regex patterns never match.
pub fn rule_matches_url(rule: &SpaceRouteRule, url: &str) -> bool {
    let pattern = rule.pattern.trim();
    if pattern.is_empty() {
        return false;
    }
    match rule.match_type {
        RouteMatchType::Contains => url.to_ascii_lowercase().contains(&pattern.to_ascii_lowercase()),
        RouteMatchType::EqualTo => normalize_url_for_equality(url) == normalize_url_for_equality(pattern),
        RouteMatchType::Regex => RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .is_ok_and(|re| re.is_match(url)),
    }
}

/// Decides which [`RouteTarget`] a URL should open in.
///
/// Rules are evaluated in order and the first matching rule wins. When no rule matches, external links use
/// `settings.default_external` and in-browser navigations stay in [`RouteTarget::MostRecentSpace`].
pub fn route_url(settings: &SpaceRoutingSettings, url: &str, from_external: bool) -> RouteTarget {
    for rule in &settings.rules {
        if rule_matches_url(rule, url) {
            return rule.target;
        }
    }
    if from_external {
        settings.default_external
    } else {
        RouteTarget::MostRecentSpace
    }
}

/// Resolves `url` to an existing [`SpaceId`], or `None` when it stays in the most recent space or the target space no
/// longer exists.
pub fn route_url_to_existing_space(
    settings: &SpaceRoutingSettings,
    spaces: &[Space],
    url: &str,
    from_external: bool,
) -> Option<SpaceId> {
    match route_url(settings, url, from_external) {
        RouteTarget::MostRecentSpace => None,
        RouteTarget::Space(space_id) => spaces.iter().any(|s| s.id == space_id).then_some(space_id),
    }
}

/// Filters `rules` by a search query matching the pattern, match type label, or destination space name.
pub fn filter_rules<'a>(rules: &'a [SpaceRouteRule], spaces: &[Space], query: &str) -> Vec<&'a SpaceRouteRule> {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return rules.iter().collect();
    }
    rules
        .iter()
        .filter(|rule| {
            if rule.pattern.to_ascii_lowercase().contains(&needle)
                || rule.match_type.label().to_ascii_lowercase().contains(&needle)
            {
                return true;
            }
            let target_name = match rule.target {
                RouteTarget::MostRecentSpace => "most recent space",
                RouteTarget::Space(id) => spaces
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| s.name.as_str())
                    .unwrap_or("most recent space"),
            };
            target_name.to_ascii_lowercase().contains(&needle)
        })
        .collect()
}

/// Allocates the next unique [`SpaceRouteId`] for `settings`.
pub fn next_rule_id(settings: &SpaceRoutingSettings) -> SpaceRouteId {
    SpaceRouteId(settings.rules.iter().map(|r| r.id.0).max().unwrap_or(0) + 1)
}

/// Appends a validated routing rule to `settings`.
pub fn add_rule(
    settings: &mut SpaceRoutingSettings,
    match_type: RouteMatchType,
    pattern: String,
    target: RouteTarget,
) -> Result<SpaceRouteRule, CommandError> {
    validate_pattern(match_type, &pattern).map_err(|message| CommandError::InvalidArgument {
        command: "space_routing.add".to_string(),
        message,
    })?;
    let rule = SpaceRouteRule {
        id: next_rule_id(settings),
        match_type,
        pattern: pattern.trim().to_string(),
        target,
    };
    settings.rules.push(rule.clone());
    Ok(rule)
}

/// Updates an existing routing rule in `settings`.
pub fn update_rule(
    settings: &mut SpaceRoutingSettings,
    rule_id: SpaceRouteId,
    match_type: RouteMatchType,
    pattern: String,
    target: RouteTarget,
) -> Result<SpaceRouteRule, CommandError> {
    validate_pattern(match_type, &pattern).map_err(|message| CommandError::InvalidArgument {
        command: "space_routing.update".to_string(),
        message,
    })?;
    let Some(existing) = settings.rules.iter_mut().find(|r| r.id == rule_id) else {
        return Err(CommandError::TargetNotFound {
            command: "space_routing.update".to_string(),
            target: format!("route {}", rule_id.0),
        });
    };
    existing.match_type = match_type;
    existing.pattern = pattern.trim().to_string();
    existing.target = target;
    Ok(existing.clone())
}

/// Removes a routing rule by id from `settings`.
pub fn delete_rule(settings: &mut SpaceRoutingSettings, rule_id: SpaceRouteId) -> Result<(), CommandError> {
    let Some(pos) = settings.rules.iter().position(|r| r.id == rule_id) else {
        return Err(CommandError::TargetNotFound {
            command: "space_routing.delete".to_string(),
            target: format!("route {}", rule_id.0),
        });
    };
    settings.rules.remove(pos);
    Ok(())
}

/// Moves a routing rule to `index` (clamped to the valid range).
pub fn reorder_rule(
    settings: &mut SpaceRoutingSettings,
    rule_id: SpaceRouteId,
    index: usize,
) -> Result<(), CommandError> {
    let Some(pos) = settings.rules.iter().position(|r| r.id == rule_id) else {
        return Err(CommandError::TargetNotFound {
            command: "space_routing.reorder".to_string(),
            target: format!("route {}", rule_id.0),
        });
    };
    let rule = settings.rules.remove(pos);
    let dest = index.min(settings.rules.len());
    settings.rules.insert(dest, rule);
    Ok(())
}

/// Sample routing rules used by the gallery and unit tests.
pub fn sample_routing_settings() -> SpaceRoutingSettings {
    SpaceRoutingSettings {
        rules: vec![
            SpaceRouteRule {
                id: SpaceRouteId(1),
                match_type: RouteMatchType::Contains,
                pattern: "github.com/planckify-labs".to_string(),
                target: RouteTarget::Space(SpaceId(2)),
            },
            SpaceRouteRule {
                id: SpaceRouteId(2),
                match_type: RouteMatchType::EqualTo,
                pattern: "zendia.local/docs".to_string(),
                target: RouteTarget::Space(SpaceId(1)),
            },
            SpaceRouteRule {
                id: SpaceRouteId(3),
                match_type: RouteMatchType::Regex,
                pattern: r"^https?://(docs|spec)\.[a-z0-9-]+\.org/".to_string(),
                target: RouteTarget::Space(SpaceId(2)),
            },
            SpaceRouteRule {
                id: SpaceRouteId(4),
                match_type: RouteMatchType::Contains,
                pattern: "news.ycombinator.com".to_string(),
                target: RouteTarget::MostRecentSpace,
            },
        ],
        default_external: RouteTarget::MostRecentSpace,
    }
}

type DefaultGradientSpec = (&'static [(u8, u8, u8)], f32);

const DEFAULT_SPACE_GRADIENTS: &[DefaultGradientSpec] = &[
    (&[(46, 84, 128), (74, 62, 134)], 140.0),
    (&[(32, 108, 92), (48, 136, 120)], 150.0),
    (&[(114, 58, 94), (148, 76, 82)], 135.0),
    (&[(122, 84, 42), (146, 104, 52)], 145.0),
    (&[(62, 54, 102), (96, 64, 118)], 135.0),
];

/// Appends a new workspace to `model` with a default name and subtle gradient when omitted.
pub fn create_space(model: &mut BrowserModel, name: Option<String>, icon: Option<String>) -> Space {
    let next_id = model.spaces.iter().map(|s| s.id.0).max().unwrap_or(0) + 1;
    let resolved_name = name
        .map(|raw| raw.trim().to_string())
        .filter(|trimmed| !trimmed.is_empty())
        .unwrap_or_else(|| format!("Space {}", model.spaces.len() + 1));
    let (stops, angle_deg) = DEFAULT_SPACE_GRADIENTS[(next_id as usize) % DEFAULT_SPACE_GRADIENTS.len()];
    let space = Space {
        id: SpaceId(next_id),
        name: resolved_name,
        icon: icon.unwrap_or_default().trim().to_string(),
        gradient: SpaceGradient {
            stops: stops.to_vec(),
            angle_deg,
            opacity: 0.9,
        },
        last_active_tab: None,
    };
    model.spaces.push(space.clone());
    space
}

/// Renames `space_id` in `model`. Empty names are rejected.
pub fn rename_space(model: &mut BrowserModel, space_id: SpaceId, name: String) -> Result<(), CommandError> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err(CommandError::InvalidArgument {
            command: "space.rename".to_string(),
            message: "space name cannot be empty".to_string(),
        });
    }
    let Some(space) = model.spaces.iter_mut().find(|s| s.id == space_id) else {
        return Err(CommandError::TargetNotFound {
            command: "space.rename".to_string(),
            target: format!("space {}", space_id.0),
        });
    };
    space.name = trimmed;
    Ok(())
}

/// Updates or clears the icon of `space_id` in `model`.
pub fn set_space_icon(model: &mut BrowserModel, space_id: SpaceId, icon: String) -> Result<(), CommandError> {
    let Some(space) = model.spaces.iter_mut().find(|s| s.id == space_id) else {
        return Err(CommandError::TargetNotFound {
            command: "space.set_icon".to_string(),
            target: format!("space {}", space_id.0),
        });
    };
    space.icon = icon.trim().to_string();
    Ok(())
}

/// Moves `space_id` to `index` in `model.spaces`.
pub fn reorder_space(model: &mut BrowserModel, space_id: SpaceId, index: usize) -> Result<(), CommandError> {
    let Some(pos) = model.spaces.iter().position(|s| s.id == space_id) else {
        return Err(CommandError::TargetNotFound {
            command: "space.reorder".to_string(),
            target: format!("space {}", space_id.0),
        });
    };
    let space = model.spaces.remove(pos);
    let dest = index.min(model.spaces.len());
    model.spaces.insert(dest, space);
    Ok(())
}

/// Deletes `space_id` from `model`, reassigning its tabs and folders to a fallback space.
///
/// Fails when deleting the only remaining space. Returns the active space id after deletion.
pub fn delete_space(model: &mut BrowserModel, space_id: SpaceId) -> Result<SpaceId, CommandError> {
    if model.spaces.len() <= 1 {
        return Err(CommandError::Failed("cannot delete the last space".to_string()));
    }
    let Some(pos) = model.spaces.iter().position(|s| s.id == space_id) else {
        return Err(CommandError::TargetNotFound {
            command: "space.delete".to_string(),
            target: format!("space {}", space_id.0),
        });
    };
    model.spaces.remove(pos);
    let fallback = model.spaces[pos.min(model.spaces.len() - 1)].id;

    for tab in &mut model.tabs {
        if tab.space_id == Some(space_id) {
            tab.space_id = Some(fallback);
        }
    }
    for pinned in &mut model.pinned_tabs {
        if pinned.space_id == space_id {
            pinned.space_id = fallback;
        }
    }
    for folder in &mut model.folders {
        if folder.space_id == space_id {
            folder.space_id = fallback;
        }
    }
    for group in &mut model.split_groups {
        if group.space_id == space_id {
            group.space_id = fallback;
        }
    }

    if model.active_space == Some(space_id) {
        model.active_space = Some(fallback);
        if let Some(window) = model.windows.first_mut() {
            window.active_space = Some(fallback);
        }
    }

    Ok(model.active_space.unwrap_or(fallback))
}

/// Builds the `...` actions menu for a Space card in the library, matching Zen's `zenWorkspaceMoreActions`.
pub fn build_library_space_menu(
    model: &BrowserModel,
    space_id: SpaceId,
    _key_table: Option<&KeyTable>,
) -> Vec<ResolvedMenuItem> {
    let sid = space_id.0.to_string();
    let is_active = model.active_space == Some(space_id);
    let can_delete = model.spaces.len() > 1;
    let has_other_spaces = model.spaces.len() > 1;
    let has_normal_tabs = !model.normal_tabs_in_space(space_id).is_empty();

    let mut items = Vec::new();
    if !is_active {
        items.push(
            ResolvedMenuItem::action("zendia.library.space.switch", "Switch to Space", "space.switch")
                .with_arg("space_id", sid.clone()),
        );
        items.push(ResolvedMenuItem::separator("zendia.library.space.sep.switch"));
    }

    items.extend([
        ResolvedMenuItem::action("zendia.library.space.rename", "Change Name…", "space.start_rename")
            .with_arg("space_id", sid.clone()),
        ResolvedMenuItem::action("zendia.library.space.icon", "Change Icon…", "space.start_icon_picker")
            .with_arg("space_id", sid.clone()),
        ResolvedMenuItem::action("zendia.library.space.theme", "Theme…", "space.open_theme_picker")
            .with_arg("space_id", sid.clone()),
        ResolvedMenuItem::separator("zendia.library.space.sep.edit"),
        ResolvedMenuItem::action("zendia.library.space.unload", "Unload Space", "space.unload")
            .with_arg("space_id", sid.clone()),
        ResolvedMenuItem::action(
            "zendia.library.space.unload_others",
            "Unload All Other Spaces",
            "space.unload_others",
        )
        .with_arg("space_id", sid.clone())
        .with_enabled(has_other_spaces),
        ResolvedMenuItem::action(
            "zendia.library.space.clear_tabs",
            "Clear Unpinned Tabs",
            "space.clear_tabs",
        )
        .with_arg("space_id", sid.clone())
        .with_enabled(has_normal_tabs),
        ResolvedMenuItem::separator("zendia.library.space.sep.manage"),
        ResolvedMenuItem::action("zendia.library.space.create", "Create Space", "space.create"),
        ResolvedMenuItem::action("zendia.library.space.delete", "Delete Space", "space.delete")
            .with_arg("space_id", sid)
            .with_enabled(can_delete),
        ResolvedMenuItem::separator("zendia.library.space.sep.routing"),
        ResolvedMenuItem::action("zendia.library.space.routing", "Space Routing Settings", "library.open")
            .with_arg("section", "space-routing"),
    ]);

    items
}

/// Extracts the host portion of an HTTP(S) or schemeless URL for quick domain routing.
pub fn extract_host(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.starts_with("about:") {
        return None;
    }
    let after_scheme = trimmed.split_once("://").map(|(_, rest)| rest).unwrap_or(trimmed);
    let authority = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or("");
    let host = if host_port.starts_with('[') {
        host_port.split(']').next().map(|h| format!("{h}]")).unwrap_or_default()
    } else {
        host_port.split(':').next().unwrap_or("").to_string()
    };
    let clean = host.trim().to_ascii_lowercase();
    (!clean.is_empty()).then_some(clean)
}

fn find_arg<'a>(args: &'a [(String, String)], key: &str) -> Option<&'a str> {
    args.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn parse_optional_u64(command: &str, args: &[(String, String)], key: &str) -> Result<Option<u64>, CommandError> {
    match find_arg(args, key) {
        None => Ok(None),
        Some(raw) => raw.parse::<u64>().map(Some).map_err(|_| CommandError::InvalidArgument {
            command: command.to_string(),
            message: format!("argument `{key}` expected integer id, got `{raw}`"),
        }),
    }
}

fn parse_required_u64(command: &str, args: &[(String, String)], key: &str) -> Result<u64, CommandError> {
    parse_optional_u64(command, args, key)?.ok_or_else(|| CommandError::InvalidArgument {
        command: command.to_string(),
        message: format!("missing required argument `{key}`"),
    })
}

fn parse_required_string(command: &str, args: &[(String, String)], key: &str) -> Result<String, CommandError> {
    find_arg(args, key)
        .map(str::to_string)
        .ok_or_else(|| CommandError::InvalidArgument {
            command: command.to_string(),
            message: format!("missing required argument `{key}`"),
        })
}

/// Parses space management and space routing commands from dotted names and key/value arguments.
pub fn parse_command(name: &str, args: &[(String, String)]) -> Result<Option<Command>, CommandError> {
    let parsed = match name {
        "space.create" | "space.new" => Command::SpaceCreate {
            name: find_arg(args, "name").map(str::to_string),
            icon: find_arg(args, "icon").map(str::to_string),
        },
        "space.rename" => Command::SpaceRename {
            space_id: SpaceId(parse_required_u64(name, args, "space_id")?),
            name: parse_required_string(name, args, "name")?,
        },
        "space.start_rename" => Command::SpaceStartRename {
            space_id: SpaceId(parse_required_u64(name, args, "space_id")?),
        },
        "space.set_icon" => Command::SpaceSetIcon {
            space_id: SpaceId(parse_required_u64(name, args, "space_id")?),
            icon: find_arg(args, "icon").unwrap_or_default().to_string(),
        },
        "space.start_icon_picker" | "space.icon" => Command::SpaceStartIconPicker {
            space_id: SpaceId(parse_required_u64(name, args, "space_id")?),
        },
        "space.open_theme_picker" | "space.theme" => Command::SpaceOpenThemePicker {
            space_id: SpaceId(parse_required_u64(name, args, "space_id")?),
        },
        "space.reorder" => Command::SpaceReorder {
            space_id: SpaceId(parse_required_u64(name, args, "space_id")?),
            index: parse_required_u64(name, args, "index")? as usize,
        },
        "space.delete" | "space.close" => Command::SpaceDelete {
            space_id: SpaceId(parse_required_u64(name, args, "space_id")?),
        },
        "space.unload" => Command::SpaceUnload {
            space_id: SpaceId(parse_required_u64(name, args, "space_id")?),
        },
        "space.unload_others" => Command::SpaceUnloadOthers {
            space_id: SpaceId(parse_required_u64(name, args, "space_id")?),
        },
        "space.open_tab" => Command::SpaceOpenTab {
            tab_id: TabId(parse_required_u64(name, args, "tab_id")?),
        },
        "space_routing.add" => {
            let match_raw = find_arg(args, "match_type").unwrap_or("contains");
            let match_type = RouteMatchType::parse(match_raw).ok_or_else(|| CommandError::InvalidArgument {
                command: name.to_string(),
                message: format!("unknown match_type `{match_raw}`"),
            })?;
            let pattern = parse_required_string(name, args, "pattern")?;
            let target_raw = find_arg(args, "target").unwrap_or("most-recent-space");
            let target = RouteTarget::parse(target_raw).ok_or_else(|| CommandError::InvalidArgument {
                command: name.to_string(),
                message: format!("unknown route target `{target_raw}`"),
            })?;
            Command::SpaceRoutingAdd {
                match_type,
                pattern,
                target,
            }
        }
        "space_routing.update" => {
            let rule_id = SpaceRouteId(parse_required_u64(name, args, "rule_id")?);
            let match_raw = parse_required_string(name, args, "match_type")?;
            let match_type = RouteMatchType::parse(&match_raw).ok_or_else(|| CommandError::InvalidArgument {
                command: name.to_string(),
                message: format!("unknown match_type `{match_raw}`"),
            })?;
            let pattern = parse_required_string(name, args, "pattern")?;
            let target_raw = parse_required_string(name, args, "target")?;
            let target = RouteTarget::parse(&target_raw).ok_or_else(|| CommandError::InvalidArgument {
                command: name.to_string(),
                message: format!("unknown route target `{target_raw}`"),
            })?;
            Command::SpaceRoutingUpdate {
                rule_id,
                match_type,
                pattern,
                target,
            }
        }
        "space_routing.delete" => Command::SpaceRoutingDelete {
            rule_id: SpaceRouteId(parse_required_u64(name, args, "rule_id")?),
        },
        "space_routing.reorder" => Command::SpaceRoutingReorder {
            rule_id: SpaceRouteId(parse_required_u64(name, args, "rule_id")?),
            index: parse_required_u64(name, args, "index")? as usize,
        },
        "space_routing.set_default_external" => {
            let target_raw = parse_required_string(name, args, "target")?;
            let target = RouteTarget::parse(&target_raw).ok_or_else(|| CommandError::InvalidArgument {
                command: name.to_string(),
                message: format!("unknown route target `{target_raw}`"),
            })?;
            Command::SpaceRoutingSetDefaultExternal { target }
        }
        "space_routing.start_edit" => Command::SpaceRoutingStartEdit {
            rule_id: SpaceRouteId(parse_required_u64(name, args, "rule_id")?),
        },
        "space_routing.add_domain" => Command::SpaceRoutingAddDomain {
            tab_id: parse_optional_u64(name, args, "tab_id")?.map(TabId),
        },
        _ => return Ok(None),
    };
    Ok(Some(parsed))
}

/// Command descriptors for space management and URL space routing commands.
pub fn command_descriptors() -> Vec<CommandDescriptor> {
    vec![
        CommandDescriptor {
            name: "space.create".to_string(),
            title: "Create Space".to_string(),
            description: "Create a new workspace".to_string(),
            arguments: vec![
                ArgumentSchema::new("name", ArgumentType::String, false, "Optional space name"),
                ArgumentSchema::new("icon", ArgumentType::String, false, "Optional icon name"),
            ],
        },
        CommandDescriptor {
            name: "space.rename".to_string(),
            title: "Rename Space".to_string(),
            description: "Set a new name for a workspace".to_string(),
            arguments: vec![
                ArgumentSchema::new("space_id", ArgumentType::Integer, true, "Target space id"),
                ArgumentSchema::new("name", ArgumentType::String, true, "New space name"),
            ],
        },
        CommandDescriptor {
            name: "space.start_rename".to_string(),
            title: "Start Renaming Space".to_string(),
            description: "Focus the inline rename field on a space card".to_string(),
            arguments: vec![ArgumentSchema::new(
                "space_id",
                ArgumentType::Integer,
                true,
                "Target space id",
            )],
        },
        CommandDescriptor {
            name: "space.set_icon".to_string(),
            title: "Set Space Icon".to_string(),
            description: "Change or clear a workspace's icon".to_string(),
            arguments: vec![
                ArgumentSchema::new("space_id", ArgumentType::Integer, true, "Target space id"),
                ArgumentSchema::new("icon", ArgumentType::String, false, "Icon name (empty to clear)"),
            ],
        },
        CommandDescriptor {
            name: "space.start_icon_picker".to_string(),
            title: "Change Space Icon".to_string(),
            description: "Open the icon picker for a workspace".to_string(),
            arguments: vec![ArgumentSchema::new(
                "space_id",
                ArgumentType::Integer,
                true,
                "Target space id",
            )],
        },
        CommandDescriptor {
            name: "space.open_theme_picker".to_string(),
            title: "Edit Space Theme".to_string(),
            description: "Open the theme gradient picker for a workspace".to_string(),
            arguments: vec![ArgumentSchema::new(
                "space_id",
                ArgumentType::Integer,
                true,
                "Target space id",
            )],
        },
        CommandDescriptor {
            name: "space.reorder".to_string(),
            title: "Reorder Space".to_string(),
            description: "Move a workspace to another slot".to_string(),
            arguments: vec![
                ArgumentSchema::new("space_id", ArgumentType::Integer, true, "Target space id"),
                ArgumentSchema::new("index", ArgumentType::Integer, true, "Destination index"),
            ],
        },
        CommandDescriptor {
            name: "space.delete".to_string(),
            title: "Delete Space".to_string(),
            description: "Delete a workspace and move its tabs to a remaining space".to_string(),
            arguments: vec![ArgumentSchema::new(
                "space_id",
                ArgumentType::Integer,
                true,
                "Target space id",
            )],
        },
        CommandDescriptor {
            name: "space.unload".to_string(),
            title: "Unload Space".to_string(),
            description: "Unload tabs in the given workspace".to_string(),
            arguments: vec![ArgumentSchema::new(
                "space_id",
                ArgumentType::Integer,
                true,
                "Target space id",
            )],
        },
        CommandDescriptor {
            name: "space.unload_others".to_string(),
            title: "Unload All Other Spaces".to_string(),
            description: "Unload tabs in every workspace except the given one".to_string(),
            arguments: vec![ArgumentSchema::new(
                "space_id",
                ArgumentType::Integer,
                true,
                "Space to keep loaded",
            )],
        },
        CommandDescriptor {
            name: "space.open_tab".to_string(),
            title: "Open Tab from Space Card".to_string(),
            description: "Switch to the tab's workspace, activate it, and close the library".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, true, "Target tab")],
        },
        CommandDescriptor {
            name: "space_routing.add".to_string(),
            title: "Add Space Routing Rule".to_string(),
            description: "Add a URL rule that routes matching links to a space".to_string(),
            arguments: vec![
                ArgumentSchema::new(
                    "match_type",
                    ArgumentType::String,
                    false,
                    "contains, equal-to, or regex",
                ),
                ArgumentSchema::new("pattern", ArgumentType::String, true, "URL pattern or regex"),
                ArgumentSchema::new("target", ArgumentType::String, false, "most-recent-space or space id"),
            ],
        },
        CommandDescriptor {
            name: "space_routing.update".to_string(),
            title: "Update Space Routing Rule".to_string(),
            description: "Edit an existing URL space routing rule".to_string(),
            arguments: vec![
                ArgumentSchema::new("rule_id", ArgumentType::Integer, true, "Rule id"),
                ArgumentSchema::new("match_type", ArgumentType::String, true, "contains, equal-to, or regex"),
                ArgumentSchema::new("pattern", ArgumentType::String, true, "URL pattern or regex"),
                ArgumentSchema::new("target", ArgumentType::String, true, "most-recent-space or space id"),
            ],
        },
        CommandDescriptor {
            name: "space_routing.delete".to_string(),
            title: "Delete Space Routing Rule".to_string(),
            description: "Remove a URL space routing rule".to_string(),
            arguments: vec![ArgumentSchema::new("rule_id", ArgumentType::Integer, true, "Rule id")],
        },
        CommandDescriptor {
            name: "space_routing.reorder".to_string(),
            title: "Reorder Space Routing Rule".to_string(),
            description: "Move a routing rule to another priority slot".to_string(),
            arguments: vec![
                ArgumentSchema::new("rule_id", ArgumentType::Integer, true, "Rule id"),
                ArgumentSchema::new("index", ArgumentType::Integer, true, "Destination index"),
            ],
        },
        CommandDescriptor {
            name: "space_routing.set_default_external".to_string(),
            title: "Set Default External Route".to_string(),
            description: "Choose where links opened from outside the browser land".to_string(),
            arguments: vec![ArgumentSchema::new(
                "target",
                ArgumentType::String,
                true,
                "most-recent-space or space id",
            )],
        },
        CommandDescriptor {
            name: "space_routing.start_edit".to_string(),
            title: "Edit Space Routing Rule".to_string(),
            description: "Open the edit form for a space routing rule".to_string(),
            arguments: vec![ArgumentSchema::new("rule_id", ArgumentType::Integer, true, "Rule id")],
        },
        CommandDescriptor {
            name: "space_routing.add_domain".to_string(),
            title: "Add Route for Tab Domain".to_string(),
            description: "Create a space routing rule for a tab's host and open Space routing".to_string(),
            arguments: vec![ArgumentSchema::new("tab_id", ArgumentType::TabId, false, "Source tab")],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::sample_model;

    #[test]
    fn match_type_and_target_round_trip() {
        for mt in RouteMatchType::ALL {
            assert_eq!(RouteMatchType::parse(mt.id()), Some(mt));
        }
        assert_eq!(RouteMatchType::parse("unknown"), None);

        assert_eq!(
            RouteTarget::parse(&RouteTarget::MostRecentSpace.to_arg_string()),
            Some(RouteTarget::MostRecentSpace)
        );
        assert_eq!(
            RouteTarget::parse(&RouteTarget::Space(SpaceId(7)).to_arg_string()),
            Some(RouteTarget::Space(SpaceId(7)))
        );
        assert_eq!(RouteTarget::parse("space:42"), Some(RouteTarget::Space(SpaceId(42))));
        assert_eq!(RouteTarget::parse("not-a-space"), None);
    }

    #[test]
    fn url_normalization_strips_scheme_www_and_trailing_slash() {
        assert_eq!(
            normalize_url_for_equality("https://www.Zendia.app/docs/"),
            "zendia.app/docs"
        );
        assert_eq!(normalize_url_for_equality("http://example.org/"), "example.org");
        assert_eq!(normalize_url_for_equality("  WWW.EXAMPLE.ORG  "), "example.org");
    }

    #[test]
    fn routing_first_match_wins_and_invalid_regex_never_matches() {
        let settings = SpaceRoutingSettings {
            rules: vec![
                SpaceRouteRule {
                    id: SpaceRouteId(1),
                    match_type: RouteMatchType::Regex,
                    pattern: "[unclosed".to_string(),
                    target: RouteTarget::Space(SpaceId(99)),
                },
                SpaceRouteRule {
                    id: SpaceRouteId(2),
                    match_type: RouteMatchType::EqualTo,
                    pattern: "https://www.zendia.local/docs/".to_string(),
                    target: RouteTarget::Space(SpaceId(1)),
                },
                SpaceRouteRule {
                    id: SpaceRouteId(3),
                    match_type: RouteMatchType::Contains,
                    pattern: "zendia.local".to_string(),
                    target: RouteTarget::Space(SpaceId(2)),
                },
                SpaceRouteRule {
                    id: SpaceRouteId(4),
                    match_type: RouteMatchType::Regex,
                    pattern: r"^https://[a-z]+\.example\.org/team/\d+$".to_string(),
                    target: RouteTarget::Space(SpaceId(3)),
                },
            ],
            default_external: RouteTarget::Space(SpaceId(2)),
        };

        // Invalid regex in rule 1 is skipped; rule 2 matches before rule 3.
        assert_eq!(
            route_url(&settings, "http://zendia.local/docs", false),
            RouteTarget::Space(SpaceId(1))
        );
        // Rule 2 fails EqualTo on sub-path, so rule 3 (Contains) wins.
        assert_eq!(
            route_url(&settings, "https://zendia.local/architecture", false),
            RouteTarget::Space(SpaceId(2))
        );
        // Case-insensitive regex rule 4 matches.
        assert_eq!(
            route_url(&settings, "HTTPS://APP.EXAMPLE.ORG/team/42", false),
            RouteTarget::Space(SpaceId(3))
        );
        // No rule matches: internal navigation stays in MostRecentSpace, external uses default_external.
        assert_eq!(
            route_url(&settings, "https://unmatched.test/", false),
            RouteTarget::MostRecentSpace
        );
        assert_eq!(
            route_url(&settings, "https://unmatched.test/", true),
            RouteTarget::Space(SpaceId(2))
        );
    }

    #[test]
    fn route_url_to_existing_space_ignores_deleted_space() {
        let model = sample_model(true);
        let settings = SpaceRoutingSettings {
            rules: vec![SpaceRouteRule {
                id: SpaceRouteId(1),
                match_type: RouteMatchType::Contains,
                pattern: "ghost.test".to_string(),
                target: RouteTarget::Space(SpaceId(999)),
            }],
            default_external: RouteTarget::MostRecentSpace,
        };
        assert_eq!(
            route_url_to_existing_space(&settings, &model.spaces, "https://ghost.test/a", false),
            None
        );
    }

    #[test]
    fn rule_crud_validation_and_reordering() {
        let mut settings = SpaceRoutingSettings::default();
        assert!(
            add_rule(
                &mut settings,
                RouteMatchType::Contains,
                "   ".to_string(),
                RouteTarget::MostRecentSpace
            )
            .is_err()
        );
        assert!(
            add_rule(
                &mut settings,
                RouteMatchType::Regex,
                "(bad".to_string(),
                RouteTarget::MostRecentSpace
            )
            .is_err()
        );

        let r1 = add_rule(
            &mut settings,
            RouteMatchType::Contains,
            "  alpha.org  ".to_string(),
            RouteTarget::Space(SpaceId(1)),
        )
        .unwrap();
        let r2 = add_rule(
            &mut settings,
            RouteMatchType::EqualTo,
            "beta.org".to_string(),
            RouteTarget::Space(SpaceId(2)),
        )
        .unwrap();
        assert_eq!(r1.pattern, "alpha.org");
        assert_eq!(settings.rules.len(), 2);

        reorder_rule(&mut settings, r2.id, 0).unwrap();
        assert_eq!(settings.rules[0].id, r2.id);
        assert_eq!(settings.rules[1].id, r1.id);

        update_rule(
            &mut settings,
            r1.id,
            RouteMatchType::Regex,
            r"alpha\.(org|com)".to_string(),
            RouteTarget::MostRecentSpace,
        )
        .unwrap();
        assert_eq!(settings.rules[1].match_type, RouteMatchType::Regex);
        assert_eq!(settings.rules[1].target, RouteTarget::MostRecentSpace);

        delete_rule(&mut settings, r2.id).unwrap();
        assert_eq!(settings.rules.len(), 1);
        assert!(delete_rule(&mut settings, r2.id).is_err());
    }

    #[test]
    fn space_crud_reorder_and_last_space_guard() {
        let mut model = sample_model(true);
        assert_eq!(model.spaces.len(), 2);

        let created = create_space(&mut model, None, Some("rocket".to_string()));
        assert_eq!(created.name, "Space 3");
        assert_eq!(created.icon, "rocket");
        assert_eq!(model.spaces.len(), 3);

        rename_space(&mut model, created.id, "Research".to_string()).unwrap();
        assert_eq!(model.space(created.id).unwrap().name, "Research");
        assert!(rename_space(&mut model, created.id, "   ".to_string()).is_err());

        set_space_icon(&mut model, created.id, "book".to_string()).unwrap();
        assert_eq!(model.space(created.id).unwrap().icon, "book");

        reorder_space(&mut model, created.id, 0).unwrap();
        assert_eq!(model.spaces[0].id, created.id);

        delete_space(&mut model, SpaceId(1)).unwrap();
        delete_space(&mut model, SpaceId(2)).unwrap();
        assert_eq!(model.spaces.len(), 1);
        // Deleting the final remaining space fails cleanly.
        assert!(delete_space(&mut model, created.id).is_err());
    }

    #[test]
    fn filter_rules_matches_pattern_and_space_name() {
        let model = sample_model(true);
        let settings = sample_routing_settings();
        let by_pattern = filter_rules(&settings.rules, &model.spaces, "github");
        assert_eq!(by_pattern.len(), 1);
        let by_space = filter_rules(&settings.rules, &model.spaces, "work");
        assert_eq!(by_space.len(), 2);
        let by_recent = filter_rules(&settings.rules, &model.spaces, "recent");
        assert_eq!(by_recent.len(), 1);
    }

    #[test]
    fn extract_host_parses_urls_and_ignores_about() {
        assert_eq!(
            extract_host("https://GitHub.com:443/planckify"),
            Some("github.com".to_string())
        );
        assert_eq!(
            extract_host("http://127.0.0.1:8782/page.html"),
            Some("127.0.0.1".to_string())
        );
        assert_eq!(extract_host("about:newtab"), None);
        assert_eq!(extract_host("   "), None);
    }

    #[test]
    fn space_and_routing_commands_parse_and_sync_model_via_events() {
        use crate::BrowserCore;
        use crate::events::{Event, ViewRequest};
        use crate::library::LibrarySection;
        use crate::stub::StubBackend;

        let core = StubBackend::new();
        let mut view_model = core.model();
        let sub = core.subscribe();

        // Parse round-trip checks.
        assert_eq!(
            Command::from_name_and_args(
                "space_routing.add",
                &[
                    ("match_type".into(), "regex".into()),
                    ("pattern".into(), r"docs\.org".into()),
                    ("target".into(), "2".into()),
                ]
            )
            .unwrap(),
            Command::SpaceRoutingAdd {
                match_type: RouteMatchType::Regex,
                pattern: r"docs\.org".to_string(),
                target: RouteTarget::Space(SpaceId(2)),
            }
        );

        // Space create, rename, icon, reorder, open tab in another space.
        core.invoke(Command::SpaceCreate {
            name: Some("Lab".to_string()),
            icon: Some("rocket".to_string()),
        })
        .unwrap();
        let lab_id = core.model().spaces.last().unwrap().id;
        core.invoke(Command::SpaceRename {
            space_id: lab_id,
            name: "Research".to_string(),
        })
        .unwrap();
        core.invoke(Command::SpaceSetIcon {
            space_id: lab_id,
            icon: "book".to_string(),
        })
        .unwrap();
        core.invoke(Command::SpaceReorder {
            space_id: lab_id,
            index: 0,
        })
        .unwrap();

        // Routing add, update, reorder, set default external, delete, add_domain.
        core.invoke(Command::SpaceRoutingAdd {
            match_type: RouteMatchType::Contains,
            pattern: "example.org".to_string(),
            target: RouteTarget::Space(SpaceId(2)),
        })
        .unwrap();
        let rule_id = core.model().space_routing.rules[0].id;
        core.invoke(Command::SpaceRoutingUpdate {
            rule_id,
            match_type: RouteMatchType::EqualTo,
            pattern: "example.org/docs".to_string(),
            target: RouteTarget::Space(lab_id),
        })
        .unwrap();
        core.invoke(Command::SpaceRoutingSetDefaultExternal {
            target: RouteTarget::Space(SpaceId(2)),
        })
        .unwrap();

        // Open a tab belonging to Space 2 while Space 1 is active: switches space, activates tab, closes library.
        let work_tab = core.model().normal_tabs_in_space(SpaceId(2))[0].id;
        core.invoke(Command::SpaceOpenTab { tab_id: work_tab }).unwrap();

        // Add domain route from active tab.
        core.invoke(Command::SpaceRoutingAddDomain { tab_id: Some(work_tab) })
            .unwrap();

        let mut saw_library_close = false;
        let mut saw_library_open_routing = false;
        while let Ok(event) = sub.try_recv() {
            if let Event::ViewRequested { request, .. } = &event {
                match request {
                    ViewRequest::LibraryClose => saw_library_close = true,
                    ViewRequest::LibraryOpen {
                        section: Some(LibrarySection::SpaceRouting),
                    } => saw_library_open_routing = true,
                    _ => {}
                }
            }
            view_model.apply_event(&event);
        }
        assert_eq!(view_model, core.model());
        assert_eq!(view_model.active_space, Some(SpaceId(2)));
        assert_eq!(view_model.active_tab, Some(work_tab));
        assert_eq!(view_model.spaces[0].name, "Research");
        assert_eq!(view_model.space_routing.rules.len(), 2);
        assert!(saw_library_close);
        assert!(saw_library_open_routing);
    }
}
