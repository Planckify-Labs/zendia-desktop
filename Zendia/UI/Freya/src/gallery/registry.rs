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
use zendia_browser_core::model::{BrowserModel, ThemeScheme, WindowId};
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
    GallerySectionSpec {
        id: "window-shell",
        title: "Window shell",
        phase: 1,
        icon: "window",
        summary: "Window layout, single/multiple/collapsed toolbar layouts, and left/right vertical sidebar frame.",
        states: &[
            GalleryStateSpec {
                id: "single-left",
                label: "Single · Left",
                description: "Single toolbar inside expanded left sidebar over rounded inset page card.",
            },
            GalleryStateSpec {
                id: "single-right",
                label: "Single · Right",
                description: "Single toolbar inside expanded right sidebar with mirrored bottom actions.",
            },
            GalleryStateSpec {
                id: "multiple-left",
                label: "Multiple · Left",
                description: "Full-width top toolbar paired with expanded left vertical sidebar.",
            },
            GalleryStateSpec {
                id: "multiple-right",
                label: "Multiple · Right",
                description: "Full-width top toolbar paired with expanded right vertical sidebar.",
            },
            GalleryStateSpec {
                id: "collapsed-left",
                label: "Collapsed · Left",
                description: "Full-width top toolbar paired with collapsed icon-only left sidebar.",
            },
            GalleryStateSpec {
                id: "collapsed-right",
                label: "Collapsed · Right",
                description: "Full-width top toolbar paired with collapsed icon-only right sidebar.",
            },
        ],
    },
    GallerySectionSpec {
        id: "menus",
        title: "Menus & About",
        phase: 1,
        icon: "menu",
        summary: "Zen-styled app menu, Ladybird-styled page/link/image/media/tab context menus, <select> dropdown, and About dialog.",
        states: &[
            GalleryStateSpec {
                id: "app-menu-vim-off",
                label: "App Menu (Vim Off)",
                description: "Application menu with built-in entries, zoom row, and Vim mode switch off.",
            },
            GalleryStateSpec {
                id: "app-menu-vim-on",
                label: "App Menu (Vim On)",
                description: "Application menu reflecting Vim mode enabled in BrowserCore.",
            },
            GalleryStateSpec {
                id: "app-menu-submenu",
                label: "App Menu Submenu",
                description: "Application menu with the Help and Report submenu expanded.",
            },
            GalleryStateSpec {
                id: "context-page",
                label: "Page Context",
                description: "Ladybird page context menu with Zendia Ask, Split View, and Glance contributions.",
            },
            GalleryStateSpec {
                id: "context-editable",
                label: "Editable & Spelling",
                description: "Editable field context menu with spelling guesses and text edit actions.",
            },
            GalleryStateSpec {
                id: "context-selection",
                label: "Selected Text",
                description: "Selected text context menu with Look Up, Copy, Search, and highlight link.",
            },
            GalleryStateSpec {
                id: "context-link",
                label: "Link Context",
                description: "Link context menu with Open in New Tab/Window/Private, Copy Link, and Download.",
            },
            GalleryStateSpec {
                id: "context-image",
                label: "Image Context",
                description: "Linked image context menu with link actions and image open/copy/save actions.",
            },
            GalleryStateSpec {
                id: "context-media",
                label: "Media Context",
                description: "Playing video and paused muted audio context menus with checkable Loop.",
            },
            GalleryStateSpec {
                id: "context-tab",
                label: "Tab Context",
                description: "Zendia tab context menu rendered in Ladybird's native menu look.",
            },
            GalleryStateSpec {
                id: "select-dropdown",
                label: "Select Dropdown",
                description: "HTML <select> popup with option groups, checkmark, disabled item, and separators.",
            },
            GalleryStateSpec {
                id: "about-dialog",
                label: "About Dialog",
                description: "About Zendia dialog with emblem, version copy pill, Ladybird attribution, and links.",
            },
        ],
    },
    GallerySectionSpec {
        id: "sidebar",
        title: "Sidebar",
        phase: 2,
        icon: "sidebar",
        summary: "Vertical tabs, essentials grid and promo, pinned tabs with Clear, collapsible folders, inline tab renamer, spaces with gradient cross-fade, drag reorder, and sidebar notification.",
        states: &[
            GalleryStateSpec {
                id: "full-populated",
                label: "Full Populated",
                description: "Essentials grid (3 tiles), 2 pinned tabs, expanded UI Spec folder, collapsed Engine folder, normal tabs, and 2 spaces.",
            },
            GalleryStateSpec {
                id: "collapsed-folder-active",
                label: "Collapsed Active Child",
                description: "Collapsed Engine folder keeping its active child tab visible below the header.",
            },
            GalleryStateSpec {
                id: "tab-renamer",
                label: "Tab Renamer",
                description: "Inline rename field active on a tab row (Enter commits via tab.rename, Escape cancels).",
            },
            GalleryStateSpec {
                id: "space-work",
                label: "Space: Work",
                description: "Switched to the Work space with teal-blue backdrop gradient, briefcase header, and shared essentials.",
            },
            GalleryStateSpec {
                id: "notification-visible",
                label: "Notification",
                description: "Update Complete! sidebar notification card at the foot of the sidebar with dismiss and link rows.",
            },
            GalleryStateSpec {
                id: "empty-essentials-promo",
                label: "Essentials Promo",
                description: "Empty essentials state displaying Zen's Add to Essentials promo card above the space.",
            },
            GalleryStateSpec {
                id: "tab-states",
                label: "Tab States",
                description: "Audible, muted, loading spinner, hovered close button, placeholder favicon, and Clear separator button.",
            },
            GalleryStateSpec {
                id: "drag-reorder",
                label: "Drag Reorder",
                description: "Active drag reorder preview with translucent tab ghost and accent drop insertion bar.",
            },
            GalleryStateSpec {
                id: "collapsed-sidebar",
                label: "Collapsed Sidebar",
                description: "Collapsed icon-only sidebar showing essentials, pinned tabs, folders, normal tabs, and space switcher.",
            },
        ],
    },
    GallerySectionSpec {
        id: "address-bar",
        title: "Address bar",
        phase: 3,
        icon: "search-glass",
        summary: "Floating address bar, ranked results (search, URL, tabs, bookmarks, history, actions), Actions command palette, and Site Data Panel.",
        states: &[
            GalleryStateSpec {
                id: "floating-empty",
                label: "Floating Empty",
                description: "Centered floating address bar opened empty for a new tab showing top site suggestions.",
            },
            GalleryStateSpec {
                id: "floating-results",
                label: "Results: Architecture",
                description: "Floating address bar querying \"Architecture\" with highlighted search row and Switch to Tab row.",
            },
            GalleryStateSpec {
                id: "results-mixed",
                label: "Results: URL & Mixed",
                description: "Direct URL visit row paired with matching open tabs, bookmarks, and history entries.",
            },
            GalleryStateSpec {
                id: "actions-unfiltered",
                label: "Actions Unfiltered",
                description: "Command palette (Actions mode) with Zen global actions and active keymap labels.",
            },
            GalleryStateSpec {
                id: "actions-filtered",
                label: "Actions Filtered",
                description: "Command palette filtered by \"tab\" showing fuzzy-ranked command matches.",
            },
            GalleryStateSpec {
                id: "actions-empty",
                label: "Actions No Match",
                description: "Command palette empty state when no command matches the typed filter.",
            },
            GalleryStateSpec {
                id: "site-not-secure",
                label: "Site Panel (HTTP)",
                description: "Site Data Panel on a not-secure http:// page with action row, extensions, tracking protection, and warning badge.",
            },
            GalleryStateSpec {
                id: "site-secure-perms",
                label: "Site Panel (HTTPS)",
                description: "Site Data Panel on a secure https:// page with granted/blocked permissions, cookies, and bookmarked state.",
            },
            GalleryStateSpec {
                id: "site-internal",
                label: "Site Panel (Internal)",
                description: "Site Data Panel on an internal about:newtab page.",
            },
        ],
    },
    GallerySectionSpec {
        id: "split-glance",
        title: "Split view & Glance",
        phase: 4,
        icon: "split",
        summary: "Split view side by side, stacked and as a grid, resize handles, drop zones for a dragged tab, the sidebar's split group row, and Glance.",
        states: &[
            GalleryStateSpec {
                id: "two-pane",
                label: "Two Panes",
                description: "Two tabs side by side with the accent ring on the active pane and the group row in the sidebar.",
            },
            GalleryStateSpec {
                id: "stacked",
                label: "Stacked",
                description: "Two tabs stacked top and bottom.",
            },
            GalleryStateSpec {
                id: "grid",
                label: "Grid",
                description: "Three panes: two stacked on the left, one at full height on the right.",
            },
            GalleryStateSpec {
                id: "resize",
                label: "Resizing",
                description: "A resize handle held mid-drag with the panes at 62% and 38%.",
            },
            GalleryStateSpec {
                id: "drop-zones",
                label: "Drop Zones",
                description: "A sidebar tab dragged over the page, highlighting the right half where it would split.",
            },
            GalleryStateSpec {
                id: "group-inactive",
                label: "Group Row",
                description: "A split group drawn as one row in the sidebar while another tab is on screen.",
            },
            GalleryStateSpec {
                id: "glance-open",
                label: "Glance Open",
                description: "A link previewed in Glance over the shrunken page, with close, expand and split buttons.",
            },
            GalleryStateSpec {
                id: "glance-opening",
                label: "Glance Opening",
                description: "Glance part-way through its open animation, growing from the link.",
            },
        ],
    },
    GallerySectionSpec {
        id: "layouts",
        title: "Compact, media & toasts",
        phase: 4,
        icon: "layout-float",
        summary: "Compact mode with the sidebar hidden and revealed, media controls, the download animation and toasts.",
        states: &[
            GalleryStateSpec {
                id: "compact-hidden",
                label: "Compact Hidden",
                description: "Compact mode: the page card fills the window and the sidebar is hidden.",
            },
            GalleryStateSpec {
                id: "compact-revealed",
                label: "Compact Revealed",
                description: "Compact mode with the sidebar slid out over the page.",
            },
            GalleryStateSpec {
                id: "compact-toolbar",
                label: "Compact Toolbar",
                description: "Multiple toolbars with only the top toolbar hidden by compact mode.",
            },
            GalleryStateSpec {
                id: "media-playing",
                label: "Media Playing",
                description: "Media controls at the foot of the sidebar for a background tab.",
            },
            GalleryStateSpec {
                id: "media-expanded",
                label: "Media Hovered",
                description: "Hovered media controls showing title, artist, picture-in-picture and progress.",
            },
            GalleryStateSpec {
                id: "media-paused-muted",
                label: "Media Paused",
                description: "Paused and muted media with the controls expanded.",
            },
            GalleryStateSpec {
                id: "media-with-notification",
                label: "Media + Notice",
                description: "Media controls together with the sidebar notification card.",
            },
            GalleryStateSpec {
                id: "download-arc",
                label: "Download Arc",
                description: "A download icon mid-flight on its arc into the library button.",
            },
            GalleryStateSpec {
                id: "toast",
                label: "Toast",
                description: "Zendia's \"Copied to the clipboard.\" toast at the top corner of the page.",
            },
            GalleryStateSpec {
                id: "toast-action",
                label: "Toast + Action",
                description: "A toast with an Undo button.",
            },
            GalleryStateSpec {
                id: "toasts-stacked",
                label: "Stacked Toasts",
                description: "Two toasts stacked.",
            },
        ],
    },
    GallerySectionSpec {
        id: "vim-statusline",
        title: "Vim statusline & which-key",
        phase: 5,
        icon: "bolt",
        summary: "Animated six-cell mascot, per-mode window tint, typed keys and counts, announcements, shrunk badge, and which-key popups.",
        states: &[
            GalleryStateSpec {
                id: "normal-pending",
                label: "NORMAL 3g",
                description: "NORMAL mode (pink) with count 3, pending key g, and the goto which-key popup.",
            },
            GalleryStateSpec {
                id: "insert",
                label: "INSERT",
                description: "INSERT mode (green) with tilted antenna, caret eye, and green window frame tint.",
            },
            GalleryStateSpec {
                id: "visual",
                label: "VISUAL",
                description: "VISUAL mode (purple) with split antenna, wide eyes, and purple window frame tint.",
            },
            GalleryStateSpec {
                id: "explorer",
                label: "EXPLORER",
                description: "EXPLORER mode (blue) with sidebar-turned eyes and blue window frame tint.",
            },
            GalleryStateSpec {
                id: "toolbar",
                label: "TOOLBAR",
                description: "TOOLBAR mode (yellow) with perked-up antenna and yellow window frame tint.",
            },
            GalleryStateSpec {
                id: "window",
                label: "WINDOW",
                description: "WINDOW mode (orange) wearing NORMAL's pose in orange with orange window frame tint.",
            },
            GalleryStateSpec {
                id: "library",
                label: "LIBRARY",
                description: "LIBRARY mode (teal) with lowered eyes and teal window frame tint.",
            },
            GalleryStateSpec {
                id: "shrunk",
                label: "Shrunk Badge",
                description: "Resting shrunk corner badge after the quiet timeout (zendia.statusline.show = shrink).",
            },
            GalleryStateSpec {
                id: "which-key-leader",
                label: "Which-Key Leader",
                description: "Which-key helix popup after <Space> showing sorted mappings and colored +group entries.",
            },
            GalleryStateSpec {
                id: "which-key-subgroup",
                label: "Subgroup +ai",
                description: "Which-key subgroup popup after <Space> a (SPC » a +ai) with the mascot dodging clear.",
            },
            GalleryStateSpec {
                id: "which-key-all",
                label: "All Keymaps",
                description: "Scrollable all-keymaps view (<leader>?) listing every top-level binding in NORMAL mode.",
            },
            GalleryStateSpec {
                id: "announcement",
                label: "Announcement",
                description: "Transient announcement pill shown beside the mascot in the statusline.",
            },
            GalleryStateSpec {
                id: "ducked",
                label: "Scroll Ducked",
                description: "Statusline ducked down and faded while the page scrolls.",
            },
        ],
    },
    GallerySectionSpec {
        id: "vim-focus",
        title: "Vim focus & window",
        phase: 5,
        icon: "compass",
        summary: "Explorer cursor and selection over sidebar rows, jump hint labels, toolbar focus, and window mode over split panes.",
        states: &[
            GalleryStateSpec {
                id: "explorer-tab",
                label: "Explorer: Tab",
                description: "EXPLORER blue cursor ring over a vertical tab row in the sidebar.",
            },
            GalleryStateSpec {
                id: "explorer-selection",
                label: "Explorer: Selection",
                description: "EXPLORER cursor combined with VISUAL purple multi-row selection.",
            },
            GalleryStateSpec {
                id: "explorer-folder",
                label: "Explorer: Folder",
                description: "EXPLORER blue cursor ring resting on a sidebar folder row.",
            },
            GalleryStateSpec {
                id: "explorer-split-group",
                label: "Explorer: Split Row",
                description: "EXPLORER blue cursor ring over a split-group row in the sidebar.",
            },
            GalleryStateSpec {
                id: "hints-sidebar",
                label: "Hint Labels",
                description: "Jump hint labels assigned across essentials, pinned tabs, folders, and normal tabs.",
            },
            GalleryStateSpec {
                id: "hints-narrowed",
                label: "Hints Narrowed",
                description: "Two-letter hint labels narrowed after typing prefix \"a\".",
            },
            GalleryStateSpec {
                id: "toolbar-button",
                label: "Toolbar: Button",
                description: "TOOLBAR yellow cursor ring over a toolbar icon button.",
            },
            GalleryStateSpec {
                id: "toolbar-urlbar",
                label: "Toolbar: URL Bar",
                description: "TOOLBAR yellow pill cursor ring around the sidebar address bar.",
            },
            GalleryStateSpec {
                id: "window-grid",
                label: "Window: 3-Pane Grid",
                description: "Numbered pane chips and WINDOW orange active ring over a three-pane split grid.",
            },
            GalleryStateSpec {
                id: "window-landing",
                label: "Window: Landing Labels",
                description: "Window mode carry state showing pane-side and outer split-edge landing labels.",
            },
        ],
    },
    GallerySectionSpec {
        id: "vim-pickers",
        title: "Vim Pickers & Region",
        phase: 5,
        icon: "screenshot",
        summary: "Zendia Vim picker modal (list with sections, filtered, grid with dimmed tiles, empty) and full-page crosshair region picker.",
        states: &[
            GalleryStateSpec {
                id: "tabs-list",
                label: "Tabs List",
                description: "Tabs picker in list layout with Open Tabs, Pinned, and Essentials sections, key badges, and active cursor.",
            },
            GalleryStateSpec {
                id: "tabs-filtered",
                label: "List Filtered",
                description: "Tabs list picker filtered by typed text showing only matching items.",
            },
            GalleryStateSpec {
                id: "agents-grid",
                label: "Agents Grid",
                description: "Agents picker in 3-column grid layout with icon plates and dimmed uninstalled items.",
            },
            GalleryStateSpec {
                id: "empty-filter",
                label: "Empty Filter",
                description: "Picker empty state when no items match the typed filter query.",
            },
            GalleryStateSpec {
                id: "region-idle",
                label: "Region Idle",
                description: "Full-page crosshair region picker overlay with instruction banner.",
            },
            GalleryStateSpec {
                id: "region-dragging",
                label: "Region Dragging",
                description: "Active accent-bordered region selection cutout over the dimmed page.",
            },
            GalleryStateSpec {
                id: "vim-disabled",
                label: "Vim Mode Off",
                description: "Vim mode disabled — picker and region picker surfaces stay hidden and gated.",
            },
        ],
    },
    GallerySectionSpec {
        id: "agent-panel",
        title: "Side Panels & Agent",
        phase: 6,
        icon: "chat",
        summary: "Float/Slide/Split dock host, Markdown & tables, working ring, model card, instructions, and confirmations.",
        states: &[
            GalleryStateSpec {
                id: "conversation-float",
                label: "Conversation · Float",
                description: "Floating glass agent panel with folded steps, Markdown, code block, interactive table, and confirmation.",
            },
            GalleryStateSpec {
                id: "empty-float",
                label: "Empty · Float",
                description: "Empty floating panel with Claude Code emblem, prompt suggestions, and composer.",
            },
            GalleryStateSpec {
                id: "split-layout",
                label: "Split Layout",
                description: "Side panel docked in-flow beside the narrowed page card with 8px gap.",
            },
            GalleryStateSpec {
                id: "slide-handle",
                label: "Slide · Edge Handle",
                description: "Closed Slide-mode panel showing the agent-colored edge handle and Chat button dot.",
            },
            GalleryStateSpec {
                id: "working-state",
                label: "Working & Streaming",
                description: "Active streaming reply with 8-dash Skia working ring, rolling status text, and Stop button.",
            },
            GalleryStateSpec {
                id: "closed-pill",
                label: "Closed · Working Pill",
                description: "Panel closed while the agent works, showing the floating bottom pill and Chat button dot.",
            },
            GalleryStateSpec {
                id: "model-card",
                label: "Model & Effort Card",
                description: "Floating glass popover with color-coded effort slider and context window selector.",
            },
            GalleryStateSpec {
                id: "model-card-list",
                label: "Model Card · List",
                description: "Searchable grouped model list sub-view with capability badges and detail bar.",
            },
            GalleryStateSpec {
                id: "instructions-empty",
                label: "Instructions · Empty",
                description: "Full-panel Agent instructions overlay in its empty state.",
            },
            GalleryStateSpec {
                id: "instructions-populated",
                label: "Instructions · Cards",
                description: "Agent instructions overlay with switchable cards, file tile, and guideline editor.",
            },
            GalleryStateSpec {
                id: "declared-page-and-media",
                label: "Declared UI & Media",
                description: "Native DeclaredUi card, CAPTCHA button, staged attachment, and media compatibility warning.",
            },
            GalleryStateSpec {
                id: "service-down",
                label: "AgentServer Down",
                description: "AgentServer stopped callout with Retry button while preserving the conversation trace.",
            },
        ],
    },
    GallerySectionSpec {
        id: "agents-panel",
        title: "Agents Panel",
        phase: 7,
        icon: "agents",
        summary: "Cross-window agent roster grouped by urgency, inline permission cards, stop/peek/dismiss, and Agents badge.",
        states: &[
            GalleryStateSpec {
                id: "populated-float",
                label: "Populated · Float",
                description: "Left floating Agents panel with NEEDS YOU, WORKING, FINISHED, IDLE groups and amber badge.",
            },
            GalleryStateSpec {
                id: "split-layout",
                label: "Split Layout",
                description: "Agents roster docked in Split mode beside the narrowed page card.",
            },
            GalleryStateSpec {
                id: "empty",
                label: "Empty Roster",
                description: "Agents panel empty state ('No agents at work') with quick New chat trigger.",
            },
            GalleryStateSpec {
                id: "service-down",
                label: "AgentServer Down",
                description: "AgentServer unavailable banner in the Agents panel while preserving the roster.",
            },
        ],
    },
    GallerySectionSpec {
        id: "webchat",
        title: "Web Chat",
        phase: 8,
        icon: "webchat",
        summary: "Native web chat dock with 8-site strip, custom site bar, accounts menu, Ask submenu, and dictation state.",
        states: &[
            GalleryStateSpec {
                id: "panel-float",
                label: "Float · Site Strip",
                description: "Floating Web chat panel with header actions, 8-site brand strip, loading bar, and waiting emblem.",
            },
            GalleryStateSpec {
                id: "panel-split",
                label: "Split Layout",
                description: "Web chat panel docked in Split mode beside the narrowed page card.",
            },
            GalleryStateSpec {
                id: "account-menu",
                label: "Accounts Menu",
                description: "Accounts dropdown showing Account 1, shared browser cookie note, and login refresh.",
            },
            GalleryStateSpec {
                id: "add-site",
                label: "Add Custom Site",
                description: "Inline custom chat site URL entry bar toggled from the site strip's + button.",
            },
            GalleryStateSpec {
                id: "ask-submenu",
                label: "Ask Context Menu",
                description: "Tab context menu with the 'Ask ChatGPT About This Tab' prompts submenu.",
            },
            GalleryStateSpec {
                id: "dictation",
                label: "Dictation Listening",
                description: "Voice dictation listening indicator and pending Ask prompt notice inside the Web chat panel.",
            },
        ],
    },
    GallerySectionSpec {
        id: "library",
        title: "Library",
        phase: 9,
        icon: "library",
        summary: "Library shell, section rail, search and filter bar, History section, keyboard layer, and open slide motion.",
        states: &[
            GalleryStateSpec {
                id: "history",
                label: "History",
                description: "Library open on History with Today's visits, search box, Filter button, and section rail.",
            },
            GalleryStateSpec {
                id: "history-search",
                label: "Search Filtered",
                description: "History section filtered by the search query \"Design\".",
            },
            GalleryStateSpec {
                id: "history-filters",
                label: "Filter Panel",
                description: "Slide-down filter panel open with time range, sort order, and source chips.",
            },
            GalleryStateSpec {
                id: "history-empty",
                label: "Empty",
                description: "History section empty state when no visits are recorded.",
            },
            GalleryStateSpec {
                id: "vim-focus",
                label: "Vim Focus",
                description: "LIBRARY input mode with teal cursor ring, focused search box, teal mascot, and frame tint.",
            },
            GalleryStateSpec {
                id: "right-side",
                label: "Right Side",
                description: "Library anchored to the right sidebar edge with mirrored rail and page card pushed left.",
            },
            GalleryStateSpec {
                id: "mid-open",
                label: "Mid-Open Slide",
                description: "Library frozen partway (55%) through its open slide animation with the page card moving aside.",
            },
        ],
    },
    GallerySectionSpec {
        id: "library-spaces",
        title: "Library · Spaces",
        phase: 9,
        icon: "layers",
        summary: "Library Spaces cards (inline rename, tab drag, menu, scroll) and Space routing rules & editor.",
        states: &[
            GalleryStateSpec {
                id: "spaces-two",
                label: "Spaces · Two Cards",
                description: "Two workspace cards (Space and Research) with tabs, divider, move handle, ... menu, and + button.",
            },
            GalleryStateSpec {
                id: "spaces-scrolled",
                label: "Spaces · Scrolled",
                description: "Five workspace cards scrolled horizontally inside the Spaces strip.",
            },
            GalleryStateSpec {
                id: "spaces-renaming",
                label: "Spaces · Renaming",
                description: "Inline rename input active on the Space card with the teal focus ring.",
            },
            GalleryStateSpec {
                id: "spaces-dragging",
                label: "Spaces · Tab Drag",
                description: "Dragging a tab between workspace cards with floating ghost pill and accent drop indicator.",
            },
            GalleryStateSpec {
                id: "spaces-menu",
                label: "Spaces · Card Menu",
                description: "Card `...` actions menu open with rename, icon, theme, unload, clear, create, delete, and routing entries.",
            },
            GalleryStateSpec {
                id: "routing-rules",
                label: "Routing · Rules",
                description: "Space routing rules list with search bar, match types, target spaces, and external-links footer.",
            },
            GalleryStateSpec {
                id: "routing-empty",
                label: "Routing · Empty",
                description: "Empty state when no space routing rules have been configured yet.",
            },
            GalleryStateSpec {
                id: "routing-adding",
                label: "Routing · Add Rule",
                description: "New route form with Contains / Is Exactly / RegEx chips, URL pattern input, and space picker.",
            },
            GalleryStateSpec {
                id: "routing-invalid-regex",
                label: "Routing · Invalid Regex",
                description: "Editing a regex rule with an invalid pattern showing the error border, message, and disabled Save button.",
            },
        ],
    },
    GallerySectionSpec {
        id: "library-media",
        title: "Library · Downloads & Media",
        phase: 9,
        icon: "downloads",
        summary: "Library Downloads (empty, mixed states, filtered), Media opt-in and local thumbnail grid, and the full-size Media Preview overlay.",
        states: &[
            GalleryStateSpec {
                id: "downloads-empty",
                label: "Downloads · Empty",
                description: "Library open on Downloads with no downloads present.",
            },
            GalleryStateSpec {
                id: "downloads-mixed",
                label: "Downloads · Mixed",
                description: "Day-grouped downloads showing in-progress, paused, completed image thumbnail, completed PDF, failed, cancelled, and missing file states.",
            },
            GalleryStateSpec {
                id: "downloads-filtered",
                label: "Downloads · Filtered",
                description: "Downloads filtered by the search query \"nightly\" matching both URL and filename.",
            },
            GalleryStateSpec {
                id: "media-empty",
                label: "Media · Empty",
                description: "Media section opt-in card ('Your Media in the Library') floating over the tiled skeleton backdrop.",
            },
            GalleryStateSpec {
                id: "media-grid",
                label: "Media · Grid",
                description: "3-column grid of local PNG image thumbnails and a video tile with play badge and duration pill.",
            },
            GalleryStateSpec {
                id: "media-preview",
                label: "Media · Preview",
                description: "Full-size media preview overlay open on a local PNG image at POPUP_OVERLAY_LEVEL.",
            },
            GalleryStateSpec {
                id: "media-preview-next",
                label: "Media · Preview (Next)",
                description: "Media preview stepped forward with the right arrow onto the video item.",
            },
        ],
    },
    GallerySectionSpec {
        id: "onboarding",
        title: "Onboarding",
        phase: 10,
        icon: "sparkles",
        summary: "Full-window native onboarding across all seven pages, Living Emblem mascot, 2D scenes, mid-transition motion, and procedural audio.",
        states: &[
            GalleryStateSpec {
                id: "start",
                label: "Start",
                description: "Living Emblem start page with Voronoi mascot in NORMAL pose and six interactive mode pills.",
            },
            GalleryStateSpec {
                id: "start-explorer",
                label: "Start · Blue",
                description: "Start page with the EXPLORER mode pill picked and the Living Emblem morphed into blue.",
            },
            GalleryStateSpec {
                id: "keys",
                label: "01 Keys",
                description: "Keys chapter with standard shortcuts selected and dimmed keycap bar.",
            },
            GalleryStateSpec {
                id: "keys-vim-on",
                label: "01 Vim On",
                description: "Keys chapter with Vim mode enabled, active Which-Key popup, mascot statusline, and tactile keycap bar.",
            },
            GalleryStateSpec {
                id: "agents",
                label: "02 Agent",
                description: "Agent chapter with Claude Code selected, installed list, Cursor CLI install hint, and live agent dock preview.",
            },
            GalleryStateSpec {
                id: "agents-codex",
                label: "02 Codex",
                description: "Agent chapter switched to Codex with teal agent dock, tab ring, and cursor preview.",
            },
            GalleryStateSpec {
                id: "shield-off",
                label: "03 Shield Off",
                description: "Shield chapter with ad blocking turned off showing banner and popup ads landing over the page.",
            },
            GalleryStateSpec {
                id: "shield-on",
                label: "03 Shield On",
                description: "Shield chapter with uBlock Origin enabled showing the yellow visor shield and barrier deflecting ads.",
            },
            GalleryStateSpec {
                id: "move-in",
                label: "04 Move in",
                description: "Move in chapter with Start fresh, Google search, and Leave links as they are.",
            },
            GalleryStateSpec {
                id: "move-in-import",
                label: "04 Import",
                description: "Move in chapter importing Chrome's Work profile with the curved transfer arc and bookmark/history/key tokens.",
            },
            GalleryStateSpec {
                id: "move-in-search",
                label: "04 Search",
                description: "Move in chapter focused on DuckDuckGo with the floating Zen pill address bar preview.",
            },
            GalleryStateSpec {
                id: "move-in-default",
                label: "04 Default",
                description: "Move in chapter set to Open in Zendia with a link flying from another app into Zendia.",
            },
            GalleryStateSpec {
                id: "apps",
                label: "05 Apps",
                description: "Apps chapter with the 3x3 Essential app picker grid and empty sidebar preview slots.",
            },
            GalleryStateSpec {
                id: "apps-selected",
                label: "05 Pinned",
                description: "Apps chapter with Obsidian, Discord, GitHub, and Figma pinned into the sidebar preview.",
            },
            GalleryStateSpec {
                id: "go",
                label: "06 Go",
                description: "Go finale with standard shortcuts, Claude Code, and Ads allowed summary chips.",
            },
            GalleryStateSpec {
                id: "go-configured",
                label: "06 Full",
                description: "Go finale reflecting Vim keys on, Codex, uBlock shield on, 4 pinned apps, and the Vim shortcut hint card.",
            },
            GalleryStateSpec {
                id: "transition",
                label: "Transition",
                description: "Chapter step mid-transition (45% directional slide) with the companion mascot mid-jelly morph.",
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
        stub.replace_model(crate::gallery::sections::gallery_sample_model());
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
        Command::AddressBarOpen { new_tab, initial_query } => {
            format!("new_tab={new_tab} query={initial_query:?}")
        }
        Command::AddressBarSubmit { input, new_tab } => format!("\"{input}\" new_tab={new_tab}"),
        Command::AddressBarSwitchToTab { tab_id } => format!("tab={}", tab_id.0),
        Command::PaletteOpen { initial_query } => format!("query={initial_query:?}"),
        Command::SitePermissionSet {
            origin,
            permission,
            state,
        } => format!("{}={} origin={origin:?}", permission.as_str(), state.as_str()),
        Command::SiteTrackingProtectionToggle { origin } | Command::SiteClearData { origin } => {
            format!("origin={origin:?}")
        }
        Command::SiteBookmarkToggle { tab_id }
        | Command::SiteScreenshot { tab_id }
        | Command::SiteShare { tab_id }
        | Command::SiteEditStyle { tab_id }
        | Command::SiteReaderModeToggle { tab_id } => format!("tab={tab_id:?}"),
        Command::SettingsSetSearchEngine { name, search_url } => format!("{name} ({search_url})"),
        Command::SpaceSwitch { space_id } => format!("space={}", space_id.0),
        Command::SpaceClearTabs { space_id } => format!("space={space_id:?}"),
        Command::TabPin { tab_id }
        | Command::TabUnpin { tab_id }
        | Command::TabTogglePin { tab_id }
        | Command::TabAddToEssentials { tab_id }
        | Command::TabRemoveFromEssentials { tab_id }
        | Command::TabToggleEssential { tab_id }
        | Command::TabToggleMute { tab_id }
        | Command::TabDuplicate { tab_id }
        | Command::TabStartRename { tab_id } => format!("tab={tab_id:?}"),
        Command::TabRename { tab_id, title } => format!("tab={tab_id:?} title=\"{title}\""),
        Command::TabMove { tab_id, target } => format!("tab={} -> {target:?}", tab_id.0),
        Command::TabMoveToFolder { tab_id, folder_id } => format!("tab={tab_id:?} folder={folder_id:?}"),
        Command::TabMoveToSpace { tab_id, space_id } => format!("tab={tab_id:?} space={}", space_id.0),
        Command::FolderCreate { name, space_id, .. } => format!("name={name:?} space={space_id:?}"),
        Command::FolderRename { folder_id, name } => format!("folder={} name=\"{name}\"", folder_id.0),
        Command::FolderStartRename { folder_id }
        | Command::FolderDelete { folder_id }
        | Command::FolderUnpack { folder_id }
        | Command::FolderToggleCollapsed { folder_id } => format!("folder={}", folder_id.0),
        Command::NotificationDismiss { notification_id } => format!("notification={}", notification_id.0),
        Command::SplitAdd { tab_id, direction } => format!("tab={} {direction:?}", tab_id.0),
        Command::PanelToggle { panel_id } => panel_id.as_str().to_string(),
        Command::AgentSend { prompt, .. } => prompt.clone(),
        Command::AgentDismiss { conversation_id } => format!("conversation={}", conversation_id.0),
        Command::WebChatSelectSite { site_id } | Command::WebChatRemoveSite { site_id } => site_id.0.clone(),
        Command::WebChatAddSite { url, .. } => url.clone(),
        Command::WebChatSetDictating { listening } => format!("listening={listening}"),
        Command::WebChatAsk { site_id, prompt } => format!("{}: {prompt}", site_id.0),
        Command::ThemeSet { scheme, .. } => format!("scheme={scheme:?}"),
        Command::LayoutSetToolbarLayout { layout } => format!("layout={layout:?}"),
        Command::LayoutSetCompactMode { enabled } => format!("enabled={enabled}"),
        Command::SidebarSetExpanded { expanded } => format!("expanded={expanded}"),
        Command::SidebarSetSide { side } => format!("side={side:?}"),
        Command::SidebarSetWidth { width } => format!("width={width:.0}"),
        Command::VimSetEnabled { enabled } => format!("enabled={enabled}"),
        Command::ExplorerMove { direction } | Command::ToolbarFocusMove { direction } => {
            format!("direction={}", direction.as_str())
        }
        Command::ExplorerSelect { range } => format!("range={range}"),
        Command::FocusHintInput { key } => format!("key={key:?}"),
        Command::WindowModeFocusPane { pane } => format!("pane={pane}"),
        Command::WindowModeMovePane { direction } => format!("direction={}", direction.as_str()),
        Command::PageCopyLink { url } => url.clone(),
        Command::PageOpenLinkInNewTab { url, activate, .. } => format!("{url} activate={activate}"),
        Command::PageReplaceMisspelling { replacement, .. } => replacement.clone(),
        Command::PageIgnoreMisspelling { word, .. } | Command::PageLearnMisspelling { word, .. } => word.clone(),
        Command::MediaContextAction { action, .. } => action.as_str().to_string(),
        Command::PageSelectDropdownChoose { item_id, .. } => format!("item_id={item_id:?}"),
        Command::PickerOpen { name, request_id } => format!("name={name:?} request_id={request_id:?}"),
        Command::PickerChoose { request_id, item_id } => format!("request_id={request_id:?} item={item_id}"),
        Command::PickerClose { request_id } => format!("request_id={request_id:?}"),
        Command::RegionPick { tab_id } => format!("tab={tab_id:?}"),
        Command::RegionCapture {
            tab_id,
            rect,
            whole_page,
        } => format!("tab={tab_id:?} whole_page={whole_page} rect={rect:?}"),
        Command::RegionCancel => "cancelled".to_string(),
        Command::OnboardingGoto { page } => format!("page={}", page.as_str()),
        Command::OnboardingSetVim { enabled }
        | Command::OnboardingSetShield { enabled }
        | Command::OnboardingSetDefaultBrowser { enabled }
        | Command::ShieldSetEnabled { enabled }
        | Command::ImportSetDefaultBrowser { enabled } => format!("enabled={enabled}"),
        Command::OnboardingSetAgent { agent_id } => format!("agent={agent_id}"),
        Command::OnboardingSetImport { source_key, profile_id } => {
            format!("source={source_key:?} profile={profile_id:?}")
        }
        Command::OnboardingSetSearchEngine { name } => format!("engine={name}"),
        Command::OnboardingToggleEssential { url } => url.clone(),
        Command::ImportStart { source_key, profile_id } => format!("source={source_key} profile={profile_id:?}"),
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

impl GalleryCore {
    /// Runs `command` through `invoke` and records the outcome in the stub log.
    fn invoke_logged(&self, command: Command, invoke: impl FnOnce(Command) -> CommandResult) -> CommandResult {
        let name = command.name().to_string();
        let detail = format_command_detail(&command);
        let result = invoke(command);
        let outcome = match &result {
            Ok(()) => CommandLogOutcome::Handled,
            Err(CommandError::UnknownCommand(_)) => CommandLogOutcome::Unhandled,
            Err(error) => CommandLogOutcome::Error(error.to_string()),
        };
        self.record(&name, detail, outcome);
        result
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
        self.invoke_logged(command, |command| self.stub.invoke(command))
    }

    fn invoke_in(&self, window: WindowId, command: Command) -> CommandResult {
        self.invoke_logged(command, |command| self.stub.invoke_in(window, command))
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
