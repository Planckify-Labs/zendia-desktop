/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Gallery section renderers and shared visual framing helpers.

pub mod address_bar;
pub mod agent_panel;
pub mod agents_panel;
pub mod declared;
pub mod icons;
pub mod keymap;
pub mod kit;
pub mod layouts;
pub mod menus;
pub mod page;
pub mod sidebar;
pub mod split_glance;
pub mod theme;
pub mod vim_focus;
pub mod vim_pickers;
pub mod vim_statusline;
pub mod webchat_panel;
pub mod window_shell;

use crate::gallery::registry::GalleryCore;
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;

/// Renders the active gallery section and named state.
///
/// New phases add a section arm here alongside their entry in [`crate::gallery::registry::GALLERY_SECTIONS`].
///
/// Each section is keyed by its state, so switching states mounts it afresh. A state may draw with helpers that call
/// other hooks than another state's, which Freya allows only in a new component.
pub fn render_section(section_id: &str, state_id: &str, gallery_core: &Arc<GalleryCore>) -> Element {
    match section_id {
        "theme" => theme::ThemeSection {
            state_id: state_id.to_string(),
        }
        .into_element(),
        "kit" => kit::KitSection {
            state_id: state_id.to_string(),
        }
        .into_element(),
        "icons" => icons::IconsSection {
            state_id: state_id.to_string(),
        }
        .into_element(),
        "declared" => declared::DeclaredSection {
            state_id: state_id.to_string(),
        }
        .into_element(),
        "page" => page::PageSection {
            state_id: state_id.to_string(),
            gallery_core: Arc::clone(gallery_core),
        }
        .into_element(),
        "keymap" => keymap::KeymapSection {
            state_id: state_id.to_string(),
        }
        .into_element(),
        "window-shell" => window_shell::WindowShellSection {
            state_id: state_id.to_string(),
        }
        .into_element(),
        "menus" => menus::MenusSection {
            state_id: state_id.to_string(),
        }
        .into_element(),
        "sidebar" => sidebar::SidebarSection {
            state_id: state_id.to_string(),
            gallery_core: Arc::clone(gallery_core),
        }
        .into_element(),
        "address-bar" => address_bar::AddressBarSection {
            state_id: state_id.to_string(),
            gallery_core: Arc::clone(gallery_core),
        }
        .into_element(),
        "split-glance" => split_glance::SplitGlanceSection {
            state_id: state_id.to_string(),
            gallery_core: Arc::clone(gallery_core),
        }
        .into_element(),
        "layouts" => layouts::LayoutsSection {
            state_id: state_id.to_string(),
            gallery_core: Arc::clone(gallery_core),
        }
        .into_element(),
        "vim-statusline" => vim_statusline::VimStatuslineSection {
            state_id: state_id.to_string(),
            gallery_core: Arc::clone(gallery_core),
        }
        .into_element(),
        "vim-focus" => vim_focus::VimFocusSection {
            state_id: state_id.to_string(),
            gallery_core: Arc::clone(gallery_core),
        }
        .into_element(),
        "vim-pickers" => vim_pickers::VimPickersSection {
            state_id: state_id.to_string(),
            gallery_core: Arc::clone(gallery_core),
        }
        .into_element(),
        "agent-panel" => agent_panel::AgentPanelSection::new(state_id, Arc::clone(gallery_core))
            .key(state_id)
            .into_element(),
        "agents-panel" => agents_panel::AgentsPanelSection::new(state_id, Arc::clone(gallery_core))
            .key(state_id)
            .into_element(),
        "webchat" => webchat_panel::WebChatSection::new(state_id, Arc::clone(gallery_core))
            .key(state_id)
            .into_element(),
        other => rect()
            .padding(Gaps::new_all(24.0))
            .child(label().text(format!("Unknown gallery section: {other}")))
            .into_element(),
    }
}

/// Framed card used across gallery sections to group related component states cleanly.
pub fn demo_card(title: &str, subtitle: &str, content: impl IntoElement) -> Element {
    let theme = use_theme();
    rect()
        .width(Size::fill())
        .padding(Gaps::new_all(theme.spacing.xxl))
        .spacing(theme.spacing.lg)
        .corner_radius(CornerRadius::new_all(theme.radii.card))
        .background(theme.color(ThemeRole::SurfaceRaised))
        .border(
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::BorderSubtle)),
        )
        .child(
            rect()
                .spacing(theme.spacing.xxs)
                .child(
                    label()
                        .text(title.to_string())
                        .font_size(theme.font_sizes.lg)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(theme.color(ThemeRole::Text)),
                )
                .child(
                    label()
                        .text(subtitle.to_string())
                        .font_size(theme.font_sizes.sm)
                        .color(theme.color(ThemeRole::TextMuted)),
                ),
        )
        .child(content)
        .into_element()
}

/// Small uppercase caption used above individual state samples.
pub fn state_caption(text: &str) -> Element {
    let theme = use_theme();
    label()
        .text(text.to_string())
        .font_size(theme.font_sizes.xs)
        .font_weight(FontWeight::SEMI_BOLD)
        .color(theme.color(ThemeRole::TextMuted))
        .into_element()
}

/// Colorful striped backdrop placed behind [`crate::ui::kit::GlassPanel`] previews so translucency and backdrop blur
/// are immediately visible in both themes.
pub fn glass_backdrop(content: impl IntoElement) -> Element {
    let theme = use_theme();
    let stripe_colors = [
        theme.palette.blue.with_alpha(0.55).to_color(),
        theme.palette.purple.with_alpha(0.55).to_color(),
        theme.palette.pink.with_alpha(0.55).to_color(),
        theme.palette.teal.with_alpha(0.55).to_color(),
        theme.palette.orange.with_alpha(0.55).to_color(),
    ];

    rect()
        .width(Size::fill())
        .corner_radius(CornerRadius::new_all(theme.radii.dock))
        .overflow(Overflow::Clip)
        .border(
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(theme.color(ThemeRole::BorderSubtle)),
        )
        .child(
            rect()
                .position(Position::new_absolute().top(0.0).left(0.0))
                .width(Size::percent(100.0))
                .height(Size::percent(100.0))
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .children(stripe_colors.into_iter().enumerate().map(|(index, color)| {
                    rect()
                        .key(index)
                        .width(Size::flex(1.0))
                        .height(Size::fill())
                        .background(color)
                })),
        )
        .child(
            rect()
                .width(Size::fill())
                .padding(Gaps::new_all(theme.spacing.xxl))
                .child(content),
        )
        .into_element()
}
