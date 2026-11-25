/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Phase 11 gallery section: Space creation form (empty & filled), gradient generator (0, 1, 2, 3 dots and all preset
//! pages), and horizontal space swipe (mid-swipe between spaces and swiping past the last space to add a new one).

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::core_view::use_core;
use crate::ui::page::{PageArea, PaneMode};
use crate::ui::spaces::{GradientGeneratorCard, SpaceSwipeMode, SpaceSwipeViewState, try_use_space_editing_state};
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, Rgba, ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{BrowserModel, Space, SpaceGradient, SpaceId, Tab, TabId, TabKind};
use zendia_browser_core::space_appearance::{
    ColorHarmony, GRADIENT_PRESET_PAGES, GradientPreset, SpaceAppearance, TextureKind,
};

/// Gallery section showcasing all Phase 11 spaces editing states.
#[derive(Clone)]
pub struct SpacesEditingSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
    key: DiffKey,
}

impl PartialEq for SpacesEditingSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

impl KeyExt for SpacesEditingSection {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl SpacesEditingSection {
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

    model.essentials.clear();
    model.pinned_tabs.clear();
    model.folders.clear();
    model.split_groups.clear();
    model.sidebar_notifications.clear();
    model.media_sessions.clear();
    model.show_essentials_promo = false;

    model.spaces = vec![
        Space {
            id: space1,
            name: "Space".to_string(),
            icon: String::new(),
            gradient: SpaceGradient {
                stops: vec![(102, 76, 98), (118, 86, 114)],
                angle_deg: 135.0,
                opacity: 0.72,
            },
            last_active_tab: Some(TabId(201)),
        },
        Space {
            id: space2,
            name: "Work".to_string(),
            icon: "briefcase".to_string(),
            gradient: SpaceGradient {
                stops: vec![(32, 92, 118), (48, 128, 150)],
                angle_deg: 145.0,
                opacity: 0.78,
            },
            last_active_tab: Some(TabId(206)),
        },
    ];

    model.tabs = vec![
        make_tab(201, space1, "Repository — Zendia", "https://zendia.local/code", "code"),
        make_tab(
            202,
            space1,
            "Releases — Zendia",
            "https://zendia.local/releases",
            "rocket",
        ),
        make_tab(
            203,
            space1,
            "Design Tokens — Zendia",
            "https://zendia.local/design",
            "flame",
        ),
        make_tab(
            204,
            space1,
            "Architecture — Zendia",
            "https://zendia.local/architecture",
            "code",
        ),
        make_tab(205, space1, "Zendia Documentation", "https://zendia.local/docs", "book"),
        make_tab(
            206,
            space2,
            "Sprint Board — Work",
            "https://work.zendia.local/board",
            "briefcase",
        ),
    ];

    let active_space = if state_id == "swipe-add-space" { space2 } else { space1 };
    let active_tab = if active_space == space2 { TabId(206) } else { TabId(201) };
    model.active_space = Some(active_space);
    model.active_tab = Some(active_tab);
    if let Some(win) = model.windows.first_mut() {
        win.active_space = Some(active_space);
        win.active_tab = Some(active_tab);
    }

    match state_id {
        "gradient-one-dot" => {
            if let Some(app) = SpaceAppearance::from_preset(0, 3) {
                model.space_appearances.insert(space1, app);
            }
        }
        "gradient-two-dots" => {
            let mut app = SpaceAppearance::empty();
            app.add_dot(Some((256.0, 124.0)));
            app.add_dot(None);
            app.harmony = ColorHarmony::Complementary;
            app.set_opacity(0.62);
            model.space_appearances.insert(space1, app);
        }
        "gradient-three-dots" => {
            if let Some(mut app) = SpaceAppearance::from_preset(1, 3) {
                app.set_opacity(0.66);
                app.set_texture(TextureKind::Grain, 0.50);
                model.space_appearances.insert(space1, app);
            }
        }
        "gradient-presets" => {
            if let Some(mut app) = SpaceAppearance::from_preset(1, 6) {
                app.set_opacity(0.60);
                model.space_appearances.insert(space1, app);
            }
        }
        _ => {}
    }

    model
}

impl Component for SpacesEditingSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let editing = try_use_space_editing_state();
        let location = use_state(|| "https://zendia.local/docs".to_string());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut applied_state = use_state(String::new);

        if *applied_state.peek() != self.state_id {
            let current_theme = core.model().peek().theme.clone();
            let mut model = build_fixture_model(&self.state_id);
            model.theme = current_theme;
            self.gallery_core.stub().replace_model(model);
            if let Some(ed) = editing {
                ed.reset();
                match self.state_id.as_str() {
                    "create-empty" => {
                        ed.open_create_form();
                    }
                    "create-filled" => {
                        ed.open_create_form();
                        let mut name = ed.draft_name;
                        name.set("Design".to_string());
                        let mut icon = ed.draft_icon;
                        icon.set("sparkles".to_string());
                        let mut profile = ed.draft_profile;
                        profile.set("Work".to_string());
                        if let Some(mut app) = SpaceAppearance::from_preset(1, 1) {
                            app.set_opacity(0.62);
                            let mut draft_app = ed.draft_appearance;
                            draft_app.set(app);
                        }
                    }
                    "swipe-between" => {
                        let mut sw = ed.swipe;
                        sw.set(SpaceSwipeViewState {
                            mode: SpaceSwipeMode::SwitchSpace,
                            offset_x: -64.0,
                            add_progress: 0.0,
                            target_space_id: Some(SpaceId(2)),
                            generation: 1,
                            is_preview: true,
                        });
                    }
                    "swipe-add-space" => {
                        let mut sw = ed.swipe;
                        sw.set(SpaceSwipeViewState {
                            mode: SpaceSwipeMode::AddSpace,
                            offset_x: -102.0,
                            add_progress: 0.86,
                            target_space_id: None,
                            generation: 1,
                            is_preview: true,
                        });
                    }
                    _ => {}
                }
            }
            applied_state.set(self.state_id.clone());
        }

        let (active_space_id, active_appearance) = {
            let model = core.model().read();
            let sid = model.active_space.unwrap_or(SpaceId(1));
            (sid, model.space_appearance(sid))
        };

        let show_inline_picker = matches!(
            self.state_id.as_str(),
            "gradient-empty" | "gradient-one-dot" | "gradient-two-dots" | "gradient-three-dots"
        );
        let show_presets_board = self.state_id == "gradient-presets";

        let stage_page = PageArea::new(PaneMode::Placeholder);
        let shell = WindowShell::new(location, location_a11y_id, stage_page)
            .interactive_window_controls(false)
            .key(format!("spaces-editing-shell-{}", self.state_id));

        let inline_picker = show_inline_picker.then(|| {
            rect()
                .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                .position(Position::new_absolute().left(182.0).top(68.0))
                .child(GradientGeneratorCard {
                    appearance: active_appearance.clone(),
                    space_id: Some(active_space_id),
                    for_draft: false,
                    editing,
                    key: DiffKey::from(&self.state_id),
                })
        });

        let presets_board = show_presets_board.then(|| {
            let panel_bg = match theme.scheme {
                ResolvedScheme::Dark => Rgba::from_rgb_alpha(26, 27, 32, 0.96).to_color(),
                ResolvedScheme::Light => Rgba::from_rgb_alpha(248, 248, 251, 0.97).to_color(),
            };
            rect()
                .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                .position(Position::new_absolute().left(212.0).top(44.0))
                .width(Size::px(452.0))
                .padding(Gaps::new_all(18.0))
                .spacing(12.0)
                .corner_radius(CornerRadius::new_all(14.0))
                .background(panel_bg)
                .border(
                    Border::new()
                        .width(1.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(theme.color(ThemeRole::BorderSubtle)),
                )
                .shadow(Shadow::new().y(12.0).blur(28.0).color(Color::from_argb(100, 0, 0, 0)))
                .child(
                    label()
                        .text("All Gradient Preset Palettes")
                        .font_size(15.0)
                        .font_weight(FontWeight::BOLD)
                        .color(theme.color(ThemeRole::Text)),
                )
                .child(
                    label()
                        .text("Click any swatch to apply its preset live to the space backdrop.")
                        .font_size(12.0)
                        .color(theme.color(ThemeRole::TextMuted)),
                )
                .children(GRADIENT_PRESET_PAGES.iter().enumerate().map(|(page_idx, page)| {
                    PresetPageRow {
                        page_index: page_idx,
                        label: page.label,
                        presets: page.presets,
                        active_preset: active_appearance.preset,
                        space_id: active_space_id,
                        key: DiffKey::from(&page.id),
                    }
                    .into_element()
                }))
        });

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .padding(Gaps::new_all(12.0))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .corner_radius(CornerRadius::new_all(10.0))
                    .overflow(Overflow::Clip)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(theme.color(ThemeRole::BorderSubtle)),
                    )
                    .child(shell)
                    .maybe_child(inline_picker)
                    .maybe_child(presets_board),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct PresetPageRow {
    page_index: usize,
    label: &'static str,
    presets: &'static [GradientPreset],
    active_preset: Option<(usize, usize)>,
    space_id: SpaceId,
    key: DiffKey,
}

impl KeyExt for PresetPageRow {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for PresetPageRow {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let page_index = self.page_index;
        let space_id = self.space_id;
        let active_preset = self.active_preset;

        rect()
            .width(Size::px(416.0))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .spacing(10.0)
            .child(
                rect().width(Size::px(94.0)).child(
                    label()
                        .text(self.label)
                        .font_size(12.0)
                        .font_weight(FontWeight::MEDIUM)
                        .color(theme.color(ThemeRole::TextMuted))
                        .max_lines(1),
                ),
            )
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::SpaceBetween)
                    .cross_align(Alignment::center())
                    .children(self.presets.iter().enumerate().map(|(idx, preset)| {
                        GalleryPresetDot {
                            preset: *preset,
                            selected: active_preset == Some((page_index, idx)),
                            page: page_index,
                            index: idx,
                            space_id,
                            key: DiffKey::from(&(page_index * 16 + idx)),
                        }
                        .into_element()
                    })),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct GalleryPresetDot {
    preset: GradientPreset,
    selected: bool,
    page: usize,
    index: usize,
    space_id: SpaceId,
    key: DiffKey,
}

impl KeyExt for GalleryPresetDot {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for GalleryPresetDot {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let page = self.page;
        let index = self.index;
        let space_id = self.space_id;

        let bg: Fill = if let Some([c1, _, c3]) = self.preset.preview_colors {
            LinearGradient::new()
                .angle(135.0)
                .stop((Color::from_rgb(c1.0, c1.1, c1.2), 0.0))
                .stop((Color::from_rgb(c3.0, c3.1, c3.2), 100.0))
                .into()
        } else {
            Color::from_rgb(self.preset.rgb.0, self.preset.rgb.1, self.preset.rgb.2).into()
        };

        let border_color = if self.selected {
            theme.color(ThemeRole::Accent)
        } else if hovered() {
            Color::WHITE
        } else {
            Color::from_argb(48, 0, 0, 0)
        };

        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Preset {} #{}", page + 1, index + 1))
            .width(Size::px(26.0))
            .height(Size::px(26.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .background(bg)
            .border(
                Border::new()
                    .width(if self.selected { 2.0 } else { 1.0 })
                    .alignment(BorderAlignment::Inner)
                    .fill(border_color),
            )
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                core.invoke_or_log(Command::SpaceAppearanceApplyPreset {
                    space_id: Some(space_id),
                    page,
                    index,
                });
            })
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
