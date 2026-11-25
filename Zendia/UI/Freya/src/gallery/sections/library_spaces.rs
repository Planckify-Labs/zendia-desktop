/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 9 gallery section: Library Spaces and Space routing sections inside the full window shell.

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::chrome::menus::{OpenContextMenu, try_use_menu_state};
use crate::ui::core_view::use_core;
use crate::ui::kit::{KitChip, Pill};
use crate::ui::library::sections::space_routing::{SpaceRouteDraft, try_use_space_routing_view_state};
use crate::ui::library::sections::spaces::{LibraryTabDrag, try_use_spaces_view_state};
use crate::ui::library::try_use_library_state;
use crate::ui::page::{PageArea, PaneMode};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::BrowserCore;
use zendia_browser_core::commands::Command;
use zendia_browser_core::library::LibrarySection;
use zendia_browser_core::model::{BrowserModel, Space, SpaceGradient, SpaceId, Tab, TabId, TabKind, TabMoveTarget};
use zendia_browser_core::space_routing::{
    RouteMatchType, RouteTarget, SpaceRouteId, SpaceRoutingSettings, build_library_space_menu, sample_routing_settings,
};
use zendia_keymap::KeyTable;
use zendia_module_api::ui::UiTone;

/// Gallery section showcasing all Phase 9 Library Spaces and Space routing states.
#[derive(Clone)]
pub struct LibrarySpacesSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
    key: DiffKey,
}

impl PartialEq for LibrarySpacesSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

impl KeyExt for LibrarySpacesSection {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl LibrarySpacesSection {
    pub fn new(state_id: impl Into<String>, gallery_core: Arc<GalleryCore>) -> Self {
        Self {
            state_id: state_id.into(),
            gallery_core,
            key: DiffKey::None,
        }
    }
}

fn make_tab(id: u64, space_id: SpaceId, title: &str, url: &str, favicon: &str) -> Tab {
    Tab {
        id: TabId(id),
        view_id: None,
        url: url.to_string(),
        title: title.to_string(),
        custom_title: None,
        favicon: Some(favicon.to_string()),
        is_loading: false,
        can_navigate_back: false,
        can_navigate_forward: false,
        is_audible: false,
        is_muted: false,
        kind: TabKind::Normal,
        space_id: Some(space_id),
        folder_id: None,
        split_group_id: None,
        closing: false,
    }
}

fn build_fixture_model(state_id: &str) -> BrowserModel {
    let mut model = super::gallery_sample_model();
    let space1 = SpaceId(1);
    let space2 = SpaceId(2);

    // Match the reference shots' "Space" and "Research" cards with crisp recognizable tabs.
    let two_spaces = vec![
        Space {
            id: space1,
            name: "Space".to_string(),
            icon: String::new(),
            gradient: SpaceGradient {
                stops: vec![(32, 92, 82), (42, 118, 102)],
                angle_deg: 140.0,
                opacity: 0.9,
            },
            last_active_tab: Some(TabId(101)),
        },
        Space {
            id: space2,
            name: "Research".to_string(),
            icon: "book".to_string(),
            gradient: SpaceGradient {
                stops: vec![(28, 74, 96), (44, 104, 128)],
                angle_deg: 150.0,
                opacity: 0.9,
            },
            last_active_tab: None,
        },
    ];

    let base_tabs = vec![
        make_tab(101, space1, "Repository — Zendia", "https://zendia.local/code", "code"),
        make_tab(
            102,
            space1,
            "Releases — Zendia",
            "https://zendia.local/releases",
            "rocket",
        ),
        make_tab(
            103,
            space1,
            "Design Tokens — Zendia",
            "https://zendia.local/design",
            "flame",
        ),
        make_tab(
            104,
            space1,
            "Architecture — Zendia",
            "https://zendia.local/architecture",
            "code",
        ),
        make_tab(105, space1, "Zendia Documentation", "https://zendia.local/docs", "book"),
    ];

    model.essentials.clear();
    model.pinned_tabs.clear();
    model.folders.clear();
    model.split_groups.clear();
    model.spaces = two_spaces;
    model.tabs = base_tabs;
    model.active_space = Some(space1);
    model.active_tab = Some(TabId(101));
    if let Some(win) = model.windows.first_mut() {
        win.active_space = Some(space1);
        win.active_tab = Some(TabId(101));
    }

    match state_id {
        "spaces-scrolled" => {
            let extra_spaces = [
                (
                    SpaceId(3),
                    "Engineering",
                    "terminal",
                    vec![(58, 62, 112), (82, 74, 134)],
                    vec![
                        make_tab(
                            201,
                            SpaceId(3),
                            "Ladybird Hub Protocol",
                            "https://ladybird.org/hub",
                            "terminal",
                        ),
                        make_tab(
                            202,
                            SpaceId(3),
                            "Freya 0.5.4 Layout",
                            "https://freyaui.dev/docs",
                            "code",
                        ),
                        make_tab(
                            203,
                            SpaceId(3),
                            "Cargo Workspace Graph",
                            "https://zendia.local/cargo",
                            "layers",
                        ),
                    ],
                ),
                (
                    SpaceId(4),
                    "Design",
                    "star",
                    vec![(112, 56, 92), (144, 72, 86)],
                    vec![
                        make_tab(
                            301,
                            SpaceId(4),
                            "Tokyonight Palette",
                            "https://zendia.local/theme",
                            "star",
                        ),
                        make_tab(
                            302,
                            SpaceId(4),
                            "Motion Curves",
                            "https://zendia.local/motion",
                            "sparkles",
                        ),
                    ],
                ),
                (
                    SpaceId(5),
                    "Reading",
                    "compass",
                    vec![(110, 80, 44), (138, 102, 54)],
                    vec![make_tab(
                        401,
                        SpaceId(5),
                        "Browser Architecture Notes",
                        "https://zendia.local/notes",
                        "book",
                    )],
                ),
            ];
            model.tabs.push(make_tab(
                106,
                space2,
                "Papers & Benchmarks",
                "https://zendia.local/papers",
                "book",
            ));
            for (sid, name, icon, stops, mut extra_tabs) in extra_spaces {
                let first_tab = extra_tabs.first().map(|t| t.id);
                model.spaces.push(Space {
                    id: sid,
                    name: name.to_string(),
                    icon: icon.to_string(),
                    gradient: SpaceGradient {
                        stops,
                        angle_deg: 140.0,
                        opacity: 0.9,
                    },
                    last_active_tab: first_tab,
                });
                model.tabs.append(&mut extra_tabs);
            }
        }
        "spaces-dragging" => {
            model.tabs.push(make_tab(
                106,
                space2,
                "Benchmarks — Zendia",
                "https://zendia.local/bench",
                "compass",
            ));
            model.tabs.push(make_tab(
                107,
                space2,
                "Reading List",
                "https://zendia.local/reading",
                "book",
            ));
        }
        "routing-empty" => {
            model.space_routing = SpaceRoutingSettings::default();
        }
        "routing-rules" | "routing-adding" | "routing-invalid-regex" => {
            model.space_routing = sample_routing_settings();
        }
        _ => {}
    }

    model
}

impl Component for LibrarySpacesSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let library_state = try_use_library_state();
        let spaces_state = try_use_spaces_view_state();
        let routing_state = try_use_space_routing_view_state();
        let menu_state = try_use_menu_state();
        let location = use_state(|| "https://zendia.local/docs".to_string());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut applied_state = use_state(String::new);

        if *applied_state.peek() != self.state_id {
            applied_state.set(self.state_id.clone());
            let mut fixture = build_fixture_model(&self.state_id);
            fixture.theme = self.gallery_core.model().theme;
            let key_table = KeyTable::standard();
            let space_menu_items = build_library_space_menu(&fixture, SpaceId(1), Some(&key_table));
            self.gallery_core.stub().replace_model(fixture);

            let is_routing = self.state_id.starts_with("routing-");
            if let Some(lib) = library_state {
                lib.open(Some(if is_routing {
                    LibrarySection::SpaceRouting
                } else {
                    LibrarySection::Spaces
                }));
            }
            if let Some(ss) = spaces_state {
                ss.reset();
                match self.state_id.as_str() {
                    "spaces-renaming" => {
                        ss.start_rename(SpaceId(1), "Space");
                    }
                    "spaces-scrolled" => {
                        ss.request_scroll_to_end();
                    }
                    "spaces-dragging" => {
                        let mut drag = ss.tab_drag;
                        drag.set(Some(LibraryTabDrag {
                            tab_id: TabId(102),
                            tab_title: "Releases — Zendia".to_string(),
                            tab_favicon: Some("rocket".to_string()),
                            source_space: Some(SpaceId(1)),
                            pointer_x: 282.0,
                            pointer_y: 142.0,
                            target_space: Some(SpaceId(2)),
                            target: Some(TabMoveTarget::Normal {
                                space_id: Some(SpaceId(2)),
                                index: 1,
                            }),
                            is_preview: true,
                        }));
                    }
                    _ => {}
                }
            }
            if let Some(rs) = routing_state {
                rs.reset();
                match self.state_id.as_str() {
                    "routing-adding" => {
                        rs.set_draft(
                            SpaceRouteDraft {
                                id: None,
                                match_type: RouteMatchType::Contains,
                                target: RouteTarget::Space(SpaceId(2)),
                            },
                            "github.com/planckify-labs",
                        );
                    }
                    "routing-invalid-regex" => {
                        rs.set_draft(
                            SpaceRouteDraft {
                                id: Some(SpaceRouteId(3)),
                                match_type: RouteMatchType::Regex,
                                target: RouteTarget::Space(SpaceId(2)),
                            },
                            "^https://(docs|spec[unclosed",
                        );
                    }
                    _ => {}
                }
            }
            if let Some(ms) = menu_state {
                if self.state_id == "spaces-menu" {
                    ms.open_context_menu(OpenContextMenu {
                        position: (560.0, 560.0),
                        items: space_menu_items,
                        tab_id: None,
                        open_submenu: None,
                    });
                } else {
                    ms.close_all();
                }
            }
        }

        let status_label = match self.state_id.as_str() {
            "spaces-scrolled" => "Spaces · 5 workspace cards scrolled horizontally",
            "spaces-renaming" => "Spaces · inline rename field with teal focus ring",
            "spaces-dragging" => "Spaces · dragging a tab between cards with ghost & drop bar",
            "spaces-menu" => "Spaces · card `...` actions menu open",
            "routing-rules" => "Space routing · populated URL rules & external-links footer",
            "routing-empty" => "Space routing · empty state prompt",
            "routing-adding" => "Space routing · add route form with match chips & space picker",
            "routing-invalid-regex" => "Space routing · edit form rejecting an invalid regular expression",
            _ => "Spaces · two workspace cards with tabs, footer controls & + button",
        };

        let spaces_core = core.clone();
        let routing_core = core.clone();
        let create_core = core.clone();
        let active_section = library_state
            .map(|s| *s.section.read())
            .unwrap_or(LibrarySection::Spaces);

        let controls = rect()
            .width(Size::fill())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(8.0)
            .child(Pill::new(status_label).tone(UiTone::Accent))
            .child(
                KitChip::new("Spaces")
                    .icon("layers")
                    .selected(active_section == LibrarySection::Spaces)
                    .on_press(move |_| {
                        spaces_core.invoke_or_log(Command::LibraryOpen {
                            section: Some(LibrarySection::Spaces),
                        });
                    }),
            )
            .child(
                KitChip::new("Space Routing")
                    .icon("link")
                    .selected(active_section == LibrarySection::SpaceRouting)
                    .on_press(move |_| {
                        routing_core.invoke_or_log(Command::LibraryOpen {
                            section: Some(LibrarySection::SpaceRouting),
                        });
                    }),
            )
            .child(KitChip::new("New Space").icon("plus").on_press(move |_| {
                create_core.invoke_or_log(Command::SpaceCreate { name: None, icon: None });
            }));

        let shell = rect()
            .width(Size::fill())
            .height(Size::px(672.0))
            .corner_radius(CornerRadius::new_all(theme.radii.card))
            .overflow(Overflow::Clip)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(theme.color(ThemeRole::BorderSubtle)),
            )
            .child(
                WindowShell::new(location, location_a11y_id, PageArea::new(PaneMode::Placeholder))
                    .interactive_window_controls(false),
            );

        rect().width(Size::fill()).spacing(12.0).child(controls).child(shell)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
