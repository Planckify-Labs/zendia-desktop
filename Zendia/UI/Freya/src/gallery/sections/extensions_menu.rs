/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Gallery section for the extensions menu: every installed extension in one popup, uBlock Origin among them.

use crate::gallery::registry::GalleryCore;
use crate::ui::chrome::WindowShell;
use crate::ui::chrome::menus::ExtensionsMenuPanel;
use crate::ui::core_view::use_core;
use crate::ui::page::{PageArea, PaneMode};
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ThemeRole, use_theme};
use freya::prelude::*;
use std::sync::Arc;
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::{ServiceId, ServiceStatus};
use zendia_browser_core::modules::{ExtensionId, SAMPLE_EXTENSION_ID};

#[derive(Clone)]
pub struct ExtensionsMenuSection {
    pub state_id: String,
    pub gallery_core: Arc<GalleryCore>,
    key: DiffKey,
}

impl PartialEq for ExtensionsMenuSection {
    fn eq(&self, other: &Self) -> bool {
        self.state_id == other.state_id && Arc::ptr_eq(&self.gallery_core, &other.gallery_core)
    }
}

impl KeyExt for ExtensionsMenuSection {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ExtensionsMenuSection {
    pub fn new(state_id: impl Into<String>, gallery_core: Arc<GalleryCore>) -> Self {
        Self {
            state_id: state_id.into(),
            gallery_core,
            key: DiffKey::None,
        }
    }
}

impl Component for ExtensionsMenuSection {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let location = use_state(|| "https://zendia.local/docs".to_string());
        let location_a11y_id = use_hook(AccessibilityId::new_unique);
        let mut applied_state = use_state(String::new);

        if *applied_state.peek() != self.state_id {
            let current_theme = core.model().peek().theme.clone();
            let mut model = super::gallery_sample_model();
            model.theme = current_theme;
            self.gallery_core.stub().replace_model(model);
            let extension_id = ExtensionId::new(SAMPLE_EXTENSION_ID);
            let status = if self.state_id == "paused" {
                ServiceStatus::Stopped {
                    reason: Some("ExtensionServer paused (process exited)".to_string()),
                }
            } else {
                ServiceStatus::Running
            };
            core.invoke_or_log(Command::ServiceSetState {
                service: ServiceId::ExtensionServer,
                status,
            });
            core.invoke_or_log(Command::ExtensionSetToolbarPinned {
                extension_id: extension_id.clone(),
                pinned: self.state_id == "pinned",
            });
            if self.state_id == "empty" {
                core.invoke_or_log(Command::ExtensionRemove { extension_id });
            }
            applied_state.set(self.state_id.clone());
        }

        let panel = match self.state_id.as_str() {
            "button" => None,
            "row-menu" => Some(ExtensionsMenuPanel::new().row_menu(ExtensionId::new(SAMPLE_EXTENSION_ID))),
            _ => Some(ExtensionsMenuPanel::new()),
        };

        let shell = WindowShell::new(location, location_a11y_id, PageArea::new(PaneMode::Placeholder))
            .interactive_window_controls(false)
            .key(format!("extensions-shell-{}", self.state_id));

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
                    .maybe_child(panel.map(|panel| {
                        // Where the menu opens above the extensions button at the sidebar's bottom edge.
                        rect()
                            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                            .position(Position::new_absolute().left(10.0).bottom(54.0))
                            .child(panel.key(self.state_id.clone()))
                    })),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
