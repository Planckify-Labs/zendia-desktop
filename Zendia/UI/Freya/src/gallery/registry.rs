/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Extensible registry of gallery sections and named states, CLI option parsing, and a logging [`BrowserCore`]
//! decorator around [`StubBackend`] so every command invoked from the gallery is visible in the UI as well as stderr.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use zendia_browser_core::BrowserCore;
use zendia_browser_core::commands::{Command, CommandDescriptor, CommandError, CommandResult};
use zendia_browser_core::events::Subscription;
use zendia_browser_core::model::{BrowserModel, ThemeScheme};
use zendia_browser_core::stub::StubBackend;

/// One named state selectable within a gallery section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GalleryStateSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

/// Metadata for a top-level section in the gallery's left navigation list.
///
/// Later phases add their surfaces by appending a [`GallerySectionSpec`] to [`GALLERY_SECTIONS`] and handling its `id`
/// in [`crate::gallery::sections`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GallerySectionSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub phase: u8,
    pub icon: &'static str,
    pub summary: &'static str,
    pub states: &'static [GalleryStateSpec],
}

/// Ordered registry of all sections shown in `zendia-gallery`.
pub const GALLERY_SECTIONS: &[GallerySectionSpec] = &[
    GallerySectionSpec {
        id: "theme",
        title: "Theme & Palette",
        phase: 0,
        icon: "palette",
        summary: "Tokyonight Moon/Day palette, semantic roles, agent brand colors, glass, radii, and typography.",
        states: &[
            GalleryStateSpec {
                id: "all",
                label: "Overview",
                description: "Palette swatches, key semantic roles, and glass preview.",
            },
            GalleryStateSpec {
                id: "palette",
                label: "Palette & Agents",
                description: "All 17 Tokyonight tokens and per-agent brand colors.",
            },
            GalleryStateSpec {
                id: "roles",
                label: "Semantic Roles",
                description: "UI, mode, Markdown, and effort-level color roles.",
            },
            GalleryStateSpec {
                id: "glass",
                label: "Glass & Scales",
                description: "Glass translucency vs solid fallback, corner radii, and font sizes.",
            },
        ],
    },
    GallerySectionSpec {
        id: "kit",
        title: "Component Kit",
        phase: 0,
        icon: "sliders",
        summary: "Every shared component in all visual states, ready for built-in surfaces and declared UI.",
        states: &[
            GalleryStateSpec {
                id: "all",
                label: "All Components",
                description: "Complete showcase of panels, controls, lists, confirmations, popovers, and motion.",
            },
            GalleryStateSpec {
                id: "surfaces",
                label: "Panels, Pills & Toasts",
                description: "GlassPanel, Pill tones/dots/icons, and Toast banners.",
            },
            GalleryStateSpec {
                id: "controls",
                label: "Controls",
                description: "IconButton (normal, hover, active, disabled), KitChip, KitSwitch, and KitSlider.",
            },
            GalleryStateSpec {
                id: "lists",
                label: "Lists, Keys & Prompts",
                description: "KeyLabel styles, ListRow cursor/selection states, and normal/danger ConfirmationCard.",
            },
            GalleryStateSpec {
                id: "overlays",
                label: "Popover & Transitions",
                description: "AnchoredPopover through Portal and Fade/Slide transitions with Replay.",
            },
        ],
    },
    GallerySectionSpec {
        id: "icons",
        title: "Icon Table",
        phase: 0,
        icon: "sparkles",
        summary: "Every mapped SVG icon in ICON_TABLE with its canonical name and semantic aliases.",
        states: &[
            GalleryStateSpec {
                id: "all",
                label: "All Icons",
                description: "Complete grid of all embedded SVG icons at 20 px in default text role.",
            },
            GalleryStateSpec {
                id: "accent",
                label: "Accent Tinted",
                description: "All icons rendered with ThemeRole::Accent to verify currentColor inheritance.",
            },
            GalleryStateSpec {
                id: "large",
                label: "Large (28 px)",
                description: "Larger rasterization preview of every embedded icon.",
            },
            GalleryStateSpec {
                id: "aliases",
                label: "Semantic Aliases",
                description: "High-level Zendia aliases mapped to their canonical ICON_TABLE entries.",
            },
        ],
    },
    GallerySectionSpec {
        id: "declared",
        title: "Declared UI",
        phase: 0,
        icon: "brackets-curly",
        summary: "Sample zendia-module-api UiNode trees rendered natively through DeclaredUi and the kit.",
        states: &[
            GalleryStateSpec {
                id: "plugin",
                label: "Plugin Panel",
                description: "Sample plugin tree with handled core commands and unhandled custom commands.",
            },
            GalleryStateSpec {
                id: "agent",
                label: "Agent Output",
                description: "Sample agent declared card with key labels, status pills, and confirmation.",
            },
        ],
    },
    GallerySectionSpec {
        id: "page",
        title: "Page Surfaces",
        phase: 0,
        icon: "split",
        summary: "PageStage placeholder rectangles (single, split, overlay) plus stub-driven InputServer overlay.",
        states: &[
            GalleryStateSpec {
                id: "single",
                label: "Single Page",
                description: "One gray page placeholder filling PageStage.",
            },
            GalleryStateSpec {
                id: "split",
                label: "Two Side by Side",
                description: "Two placeholder page surfaces side by side with active pane focus ring.",
            },
            GalleryStateSpec {
                id: "overlay",
                label: "Page + Overlay",
                description: "Placeholder page with a floating Glance-style card and InputServer overlay.",
            },
            GalleryStateSpec {
                id: "input",
                label: "InputServer Overlay",
                description: "Interactive stub InputServer events: mode pill, pending keys, and which-key popup.",
            },
        ],
    },
    GallerySectionSpec {
        id: "keymap",
        title: "Standard Keymap",
        phase: 0,
        icon: "terminal",
        summary: "Compiled standard.toml profile from Libraries/Keymap rendered with KeyLabel and live dispatch.",
        states: &[
            GalleryStateSpec {
                id: "all",
                label: "All Bindings",
                description: "Every binding in the standard browser key table.",
            },
            GalleryStateSpec {
                id: "navigation",
                label: "Tabs & Navigation",
                description: "Tab management and page navigation shortcuts.",
            },
            GalleryStateSpec {
                id: "debug",
                label: "Debug & Input",
                description: "Debug shortcuts that drive stub InputServer events.",
            },
        ],
    },
];

/// Looks up a registered gallery section by its stable `id`.
pub fn find_section(id: &str) -> Option<&'static GallerySectionSpec> {
    GALLERY_SECTIONS.iter().find(|section| section.id == id)
}

/// Looks up a named state within `section` by `state_id`.
pub fn find_state(section: &GallerySectionSpec, state_id: &str) -> Option<&'static GalleryStateSpec> {
    section.states.iter().find(|state| state.id == state_id)
}

/// Returns the first (default) state id of `section`.
pub fn default_state_id(section: &GallerySectionSpec) -> &'static str {
    section.states.first().map(|state| state.id).unwrap_or("all")
}

/// Startup configuration parsed from `zendia-gallery` CLI flags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GalleryLaunchOptions {
    pub initial_section: String,
    pub initial_state: Option<String>,
    pub initial_theme: ThemeScheme,
    pub window_width: u32,
    pub window_height: u32,
}

impl Default for GalleryLaunchOptions {
    fn default() -> Self {
        Self {
            initial_section: GALLERY_SECTIONS[0].id.to_string(),
            initial_state: None,
            initial_theme: ThemeScheme::Dark,
            window_width: 1440,
            window_height: 920,
        }
    }
}

impl GalleryLaunchOptions {
    /// Parses CLI flags (`--section <id>`, `--state <id>`, `--theme light|dark|system`, `--size WxH`).
    /// Unknown or invalid values fall back to safe defaults so the gallery always starts.
    pub fn parse_args<I>(args: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        let mut options = Self::default();
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            if let Some(value) = arg.strip_prefix("--section=") {
                options.apply_section(value);
            } else if arg == "--section"
                && let Some(value) = iter.next()
            {
                options.apply_section(&value);
            } else if let Some(value) = arg.strip_prefix("--state=") {
                options.initial_state = Some(value.to_string());
            } else if arg == "--state"
                && let Some(value) = iter.next()
            {
                options.initial_state = Some(value);
            } else if let Some(value) = arg.strip_prefix("--theme=") {
                options.apply_theme(value);
            } else if arg == "--theme"
                && let Some(value) = iter.next()
            {
                options.apply_theme(&value);
            } else if let Some(value) = arg.strip_prefix("--size=") {
                options.apply_size(value);
            } else if arg == "--size"
                && let Some(value) = iter.next()
            {
                options.apply_size(&value);
            }
        }
        options
    }

    fn apply_section(&mut self, value: &str) {
        if let Some(section) = find_section(value) {
            self.initial_section = section.id.to_string();
        }
    }

    fn apply_theme(&mut self, value: &str) {
        match value.to_ascii_lowercase().as_str() {
            "light" => self.initial_theme = ThemeScheme::Light,
            "dark" => self.initial_theme = ThemeScheme::Dark,
            "system" => self.initial_theme = ThemeScheme::System,
            _ => {}
        }
    }

    fn apply_size(&mut self, value: &str) {
        if let Some((w, h)) = value.split_once('x')
            && let (Ok(width), Ok(height)) = (w.parse::<u32>(), h.parse::<u32>())
            && width >= 640
            && height >= 480
        {
            self.window_width = width;
            self.window_height = height;
        }
    }

    /// Resolves the initial state id against the selected section, falling back to the section's default state.
    pub fn resolved_initial_state(&self) -> &'static str {
        let section = find_section(&self.initial_section).unwrap_or(&GALLERY_SECTIONS[0]);
        self.initial_state
            .as_deref()
            .and_then(|state_id| find_state(section, state_id))
            .map(|state| state.id)
            .unwrap_or_else(|| default_state_id(section))
    }

    pub fn help_text() -> String {
        let section_ids: Vec<&str> = GALLERY_SECTIONS.iter().map(|s| s.id).collect();
        format!(
            "Usage: zendia-gallery [OPTIONS]\n\n\
             Options:\n\
               --section <id>            Initial section ({})\n\
               --state <id>              Initial named state within the section\n\
               --theme <light|dark|sys>  Initial color scheme (default: dark)\n\
               --size <WxH>              Window dimensions in pixels (default: 1440x920)\n\
               -h, --help                Show this message\n",
            section_ids.join(", ")
        )
    }
}

/// Result recorded when a gallery control invokes a command through [`GalleryCore`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandLogOutcome {
    Handled,
    Unhandled,
    Error(String),
}

/// One command invocation recorded by [`GalleryCore`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandLogEntry {
    pub sequence: u64,
    pub command: String,
    pub detail: String,
    pub outcome: CommandLogOutcome,
}

const MAX_COMMAND_LOG: usize = 32;

/// Thin [`BrowserCore`] wrapper around [`StubBackend`] that forwards every call to the stub while keeping an in-memory
/// ring buffer of invoked commands (including unhandled ones) for display in the gallery's status strip.
pub struct GalleryCore {
    stub: Arc<StubBackend>,
    next_seq: AtomicU64,
    log: Mutex<VecDeque<CommandLogEntry>>,
    notifier: async_channel::Sender<CommandLogEntry>,
    updates: async_channel::Receiver<CommandLogEntry>,
}

impl GalleryCore {
    pub fn new(initial_theme: ThemeScheme) -> Self {
        let stub = Arc::new(StubBackend::new());
        let _ = stub.invoke(Command::ThemeSet {
            scheme: Some(initial_theme),
            overrides: None,
        });
        let (notifier, updates) = async_channel::unbounded();
        Self {
            stub,
            next_seq: AtomicU64::new(1),
            log: Mutex::new(VecDeque::with_capacity(MAX_COMMAND_LOG)),
            notifier,
            updates,
        }
    }

    /// Underlying stub backend, used by gallery demo controls that emit sample InputServer events.
    pub fn stub(&self) -> &Arc<StubBackend> {
        &self.stub
    }

    /// Channel delivering each newly recorded command log entry to the gallery UI.
    pub fn log_updates(&self) -> async_channel::Receiver<CommandLogEntry> {
        self.updates.clone()
    }

    /// Snapshot of recent command invocations, newest last.
    pub fn recent_log(&self) -> Vec<CommandLogEntry> {
        self.log
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .cloned()
            .collect()
    }

    fn record(&self, command: &str, detail: String, outcome: CommandLogOutcome) {
        let entry = CommandLogEntry {
            sequence: self.next_seq.fetch_add(1, Ordering::Relaxed),
            command: command.to_string(),
            detail,
            outcome,
        };
        {
            let mut log = self.log.lock().unwrap_or_else(|error| error.into_inner());
            if log.len() >= MAX_COMMAND_LOG {
                log.pop_front();
            }
            log.push_back(entry.clone());
        }
        let _ = self.notifier.try_send(entry);
    }
}

fn format_command_detail(command: &Command) -> String {
    match command {
        Command::TabNew { url, .. } => url.clone().unwrap_or_else(|| "about:newtab".to_string()),
        Command::TabClose { tab_id, force } => format!("tab={tab_id:?} force={force}"),
        Command::TabActivate { tab_id } => format!("tab={}", tab_id.0),
        Command::NavigationLoadUrl { input, .. } => input.clone(),
        Command::SpaceSwitch { space_id } => format!("space={}", space_id.0),
        Command::SplitAdd { tab_id, direction } => format!("tab={} {direction:?}", tab_id.0),
        Command::PanelToggle { panel_id } => panel_id.as_str().to_string(),
        Command::AgentSend { prompt, .. } => prompt.clone(),
        Command::WebChatAsk { site_id, prompt } => format!("{}: {prompt}", site_id.0),
        Command::ThemeSet { scheme, .. } => format!("scheme={scheme:?}"),
        Command::Custom { args, .. } => {
            if args.is_empty() {
                "no args".to_string()
            } else {
                args.iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        }
        _ => String::new(),
    }
}

impl BrowserCore for GalleryCore {
    fn model(&self) -> BrowserModel {
        self.stub.model()
    }

    fn subscribe(&self) -> Subscription {
        self.stub.subscribe()
    }

    fn invoke(&self, command: Command) -> CommandResult {
        let name = command.name().to_string();
        let detail = format_command_detail(&command);
        let result = self.stub.invoke(command);
        let outcome = match &result {
            Ok(()) => CommandLogOutcome::Handled,
            Err(CommandError::UnknownCommand(_)) => CommandLogOutcome::Unhandled,
            Err(error) => CommandLogOutcome::Error(error.to_string()),
        };
        self.record(&name, detail, outcome);
        result
    }

    fn commands(&self) -> Vec<CommandDescriptor> {
        self.stub.commands()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_sections_and_states_have_unique_non_empty_ids() {
        assert!(!GALLERY_SECTIONS.is_empty());
        let mut section_ids = HashSet::new();
        for section in GALLERY_SECTIONS {
            assert!(!section.id.is_empty());
            assert!(!section.title.is_empty());
            assert!(
                section_ids.insert(section.id),
                "duplicate gallery section id `{}`",
                section.id
            );
            assert!(
                !section.states.is_empty(),
                "gallery section `{}` must define at least one state",
                section.id
            );
            let mut state_ids = HashSet::new();
            for state in section.states {
                assert!(!state.id.is_empty());
                assert!(
                    state_ids.insert(state.id),
                    "duplicate state id `{}` in section `{}`",
                    state.id,
                    section.id
                );
            }
        }
        assert!(find_section("theme").is_some());
        assert!(find_section("kit").is_some());
        assert!(find_section("icons").is_some());
        assert!(find_section("declared").is_some());
        assert!(find_section("page").is_some());
        assert!(find_section("keymap").is_some());
        assert!(find_section("missing").is_none());
    }

    #[test]
    fn launch_options_parse_cli_flags_and_resolve_states() {
        let opts = GalleryLaunchOptions::parse_args(
            [
                "--section",
                "page",
                "--state=split",
                "--theme",
                "light",
                "--size=1600x1000",
            ]
            .into_iter()
            .map(String::from),
        );
        assert_eq!(opts.initial_section, "page");
        assert_eq!(opts.resolved_initial_state(), "split");
        assert_eq!(opts.initial_theme, ThemeScheme::Light);
        assert_eq!((opts.window_width, opts.window_height), (1600, 1000));

        // Unknown section or state falls back safely without panicking.
        let fallback = GalleryLaunchOptions::parse_args(
            ["--section=unknown", "--state=nope", "--theme=invalid", "--size=10x10"]
                .into_iter()
                .map(String::from),
        );
        assert_eq!(fallback.initial_section, GALLERY_SECTIONS[0].id);
        assert_eq!(
            fallback.resolved_initial_state(),
            default_state_id(&GALLERY_SECTIONS[0])
        );
        assert_eq!(fallback.initial_theme, ThemeScheme::Dark);
    }

    #[test]
    fn gallery_core_forwards_commands_and_logs_handled_and_unhandled() {
        let core = GalleryCore::new(ThemeScheme::Dark);
        assert_eq!(core.model().theme.scheme, ThemeScheme::Dark);

        assert!(
            core.invoke(Command::ThemeSet {
                scheme: Some(ThemeScheme::Light),
                overrides: None,
            })
            .is_ok()
        );
        assert_eq!(core.model().theme.scheme, ThemeScheme::Light);

        let err = core.invoke(Command::Custom {
            name: "plugin.demo.ping".to_string(),
            args: vec![("count".to_string(), "1".to_string())],
        });
        assert!(matches!(err, Err(CommandError::UnknownCommand(_))));

        let log = core.recent_log();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].command, "theme.set");
        assert_eq!(log[0].outcome, CommandLogOutcome::Handled);
        assert_eq!(log[1].command, "plugin.demo.ping");
        assert_eq!(log[1].detail, "count=1");
        assert_eq!(log[1].outcome, CommandLogOutcome::Unhandled);
    }
}
