/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The card that follows an extension install, in the toast corner of the page card: progress while it downloads and
//! installs, the reason and a Retry when it fails, and "was added" with Pin and Manage when it is done.
//!
//! It does not block the page: browsing goes on while a slow download runs. It moves like the toasts beside it: it pops
//! in on the toast spring ([`motion::TOAST_POP_IN`]), its text settles in again at each new stage, a badge pops onto the
//! icon when the install ends, and it fades and shrinks away ([`motion::TOAST_EXIT`]) when closed.

use crate::ui::core_view::{CoreView, use_core};
use crate::ui::extensions::prompt::{DialogButton, ExtensionTile};
use crate::ui::kit::{GlassPanel, Icon, IndeterminateBar, PopupContent, use_interactive_feedback, use_last_some};
use crate::ui::theme::{ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::modules::{ExtensionInfo, ExtensionInstall, ExtensionInstallStage};
use zendia_browser_core::settings::SettingsGroupId;

/// How long "was added" stays up when the pointer is not on it.
const INSTALLED_CARD_TIMEOUT: Duration = Duration::from_secs(8);
const CARD_WIDTH: f32 = 340.0;
const TILE_SIZE: f32 = 40.0;
const BADGE_SIZE: f32 = 18.0;
/// Where the card pops in from, and its scale then.
const ENTER_Y: f32 = -10.0;
const ENTER_SCALE: f32 = 0.92;

#[derive(Clone, PartialEq)]
pub struct ExtensionInstallCard;

impl Component for ExtensionInstallCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let (current, installed_info) = {
            let model = core.model().read();
            // The consent prompt takes the card's place while it is open.
            let install = model
                .extension_install
                .clone()
                .filter(|_| model.pending_extension_prompt.is_none());
            let info = install
                .as_ref()
                .and_then(|i| i.extension_id.as_ref())
                .and_then(|id| model.extension(id).cloned());
            (install, info)
        };
        let is_open = current.is_some();
        // A closed card keeps showing what it last showed while it leaves.
        let shown = use_last_some(&current);
        let shown_id = shown.as_ref().map(|i| i.install_id).unwrap_or(0);

        let presence = use_animation_with_dependencies(&(is_open, shown_id), move |conf, (open, _)| {
            conf.on_creation(if *open { OnCreation::Run } else { OnCreation::Finish });
            conf.on_change(OnChange::Rerun);
            if *open {
                AnimNum::new(0.0, 1.0)
                    .duration(motion::TOAST_POP_IN)
                    .ease(Ease::Out)
                    .function(motion::spring_fn())
            } else {
                AnimNum::new(1.0, 0.0)
                    .duration(motion::TOAST_EXIT)
                    .ease(Ease::In)
                    .function(motion::ease_in_fn())
            }
        });
        let progress = presence.get().value();

        let Some(install) = shown else {
            return rect().into_element();
        };
        if !is_open && progress <= 0.01 {
            return rect().into_element();
        }

        // The spring overshoots a little past 1; opacity stays within bounds.
        let opacity = progress.clamp(0.0, 1.0);
        let scale = ENTER_SCALE + (1.0 - ENTER_SCALE) * progress;
        let offset = ENTER_Y * (1.0 - progress.min(1.0));

        rect()
            .opacity(opacity)
            .scale(scale)
            .offset_y(offset)
            .interactive(if is_open { Interactive::Yes } else { Interactive::No })
            .child(InstallCardBody {
                install,
                installed_info,
            })
            .into_element()
    }
}

#[derive(Clone, PartialEq)]
struct InstallCardBody {
    install: ExtensionInstall,
    installed_info: Option<ExtensionInfo>,
}

impl Component for InstallCardBody {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let timer_gen = use_state(|| 0u64);
        let mut scheduled_for = use_state(|| None::<u64>);
        let close_fb = use_interactive_feedback(true);

        let install = &self.install;
        let install_id = install.install_id;
        let failed = matches!(install.stage, ExtensionInstallStage::Failed { .. });
        let installed = matches!(install.stage, ExtensionInstallStage::Installed { .. });

        // "Was added" closes by itself; leaving the pointer on it keeps it.
        if installed && *scheduled_for.peek() != Some(install_id) {
            scheduled_for.set(Some(install_id));
            schedule_dismiss(timer_gen, hovered, core.clone(), install_id);
        }

        // The text settles in again whenever the stage changes.
        let stage_key = stage_index(&install.stage) + install_id.wrapping_mul(8);

        let icon = self
            .installed_info
            .as_ref()
            .and_then(|info| info.icon_data.clone())
            .or_else(|| install.icon.clone());
        let name = display_name(install);
        let (title, subtitle, busy) = describe(install, &name);

        let buttons = buttons(&core, install, self.installed_info.as_ref());

        let close_core = core.clone();
        let close = rect()
            .padding(Gaps::new_all(5.0))
            .corner_radius(CornerRadius::new_all(theme.radii.pill))
            .background(close_fb.background(
                Color::TRANSPARENT,
                theme.color(ThemeRole::Hover),
                theme.color(ThemeRole::Hover),
            ))
            .scale(close_fb.scale())
            .cursor(CursorIcon::Pointer)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt("Close")
            .on_pointer_enter(close_fb.on_pointer_enter())
            .on_pointer_leave(close_fb.on_pointer_leave())
            .on_pointer_down(close_fb.on_pointer_down())
            .on_global_pointer_up(close_fb.on_pointer_up())
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                // Closing a running install only hides the card; it goes on in the background.
                close_core.invoke_or_log(Command::ExtensionInstallDismiss { install_id });
            })
            .child(Icon::new("close").size(11.0).role(ThemeRole::TextMuted));

        let leave_core = core.clone();
        let accent = if failed { ThemeRole::Danger } else { ThemeRole::Accent };

        // A rounded track under the stage text while the install runs.
        let progress_track = busy.then(|| {
            rect()
                .width(Size::fill())
                .margin(Gaps::new(7.0, 0.0, 1.0, 0.0))
                .height(Size::px(4.0))
                .corner_radius(CornerRadius::new_all(theme.radii.pill))
                .overflow(Overflow::Clip)
                .background(theme.rgba(accent).with_alpha(0.14).to_color())
                .child(IndeterminateBar::new(4.0, theme.color(accent)))
        });

        let body = rect()
            .width(Size::fill())
            .padding(Gaps::new(14.0, 12.0, 14.0, 14.0))
            .spacing(theme.spacing.xl)
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::Start)
                    .spacing(theme.spacing.xl)
                    .child(
                        rect()
                            .width(Size::px(TILE_SIZE))
                            .height(Size::px(TILE_SIZE))
                            .child(ExtensionTile {
                                cache_key: format!("install-{install_id}"),
                                name: name.clone(),
                                icon,
                                size: TILE_SIZE,
                            })
                            .maybe_child((installed || failed).then(|| {
                                rect()
                                    .position(
                                        Position::new_absolute()
                                            .left(TILE_SIZE - BADGE_SIZE + 5.0)
                                            .top(TILE_SIZE - BADGE_SIZE + 5.0),
                                    )
                                    .child(StageBadge {
                                        failed,
                                        key: install_id,
                                    })
                            })),
                    )
                    .child(
                        rect().width(Size::flex(1.0)).child(
                            PopupContent::new(stage_key).animate_on_mount(true).child(
                                rect()
                                    .width(Size::fill())
                                    .spacing(3.0)
                                    .child(
                                        label()
                                            .width(Size::fill())
                                            .text(title)
                                            .font_size(theme.font_sizes.lg)
                                            .font_weight(FontWeight::SEMI_BOLD)
                                            .max_lines(2)
                                            .text_overflow(TextOverflow::Ellipsis)
                                            .color(theme.color(ThemeRole::Text)),
                                    )
                                    .child(
                                        label()
                                            .width(Size::fill())
                                            .text(subtitle)
                                            .font_size(theme.font_sizes.sm)
                                            .max_lines(4)
                                            .color(theme.color(if failed {
                                                ThemeRole::Danger
                                            } else {
                                                ThemeRole::TextMuted
                                            })),
                                    )
                                    .maybe_child(progress_track),
                            ),
                        ),
                    )
                    .child(close),
            )
            .maybe_child((!buttons.is_empty()).then(|| {
                PopupContent::new(stage_key).animate_on_mount(true).child(
                    rect()
                        .width(Size::fill())
                        .direction(Direction::Horizontal)
                        .main_align(Alignment::End)
                        .spacing(theme.spacing.md)
                        .children(buttons),
                )
            }));

        rect()
            .a11y_role(if failed {
                AccessibilityRole::Alert
            } else {
                AccessibilityRole::Status
            })
            .on_pointer_enter(move |_| hovered.set(true))
            .on_pointer_leave(move |_| {
                hovered.set(false);
                if installed {
                    schedule_dismiss(timer_gen, hovered, leave_core.clone(), install_id);
                }
            })
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .child(
                GlassPanel::new()
                    .width(Size::px(CARD_WIDTH))
                    .padding(Gaps::new_all(0.0))
                    .spacing(0.0)
                    .corner_radius(theme.radii.card)
                    .child(body),
            )
            .into_element()
    }
}

fn buttons(core: &CoreView, install: &ExtensionInstall, installed_info: Option<&ExtensionInfo>) -> Vec<Element> {
    let install_id = install.install_id;
    let mut buttons: Vec<Element> = Vec::new();
    match &install.stage {
        stage if stage.is_running() => {
            let core = core.clone();
            buttons.push(
                DialogButton {
                    label: "Cancel".to_string(),
                    primary: false,
                }
                .on_press(move || core.invoke_or_log(Command::ExtensionInstallCancel { install_id }))
                .into_element(),
            );
        }
        ExtensionInstallStage::Failed { can_retry, .. } => {
            let dismiss_core = core.clone();
            buttons.push(
                DialogButton {
                    label: "Dismiss".to_string(),
                    primary: false,
                }
                .on_press(move || dismiss_core.invoke_or_log(Command::ExtensionInstallDismiss { install_id }))
                .into_element(),
            );
            if *can_retry {
                let retry_core = core.clone();
                buttons.push(
                    DialogButton {
                        label: "Try again".to_string(),
                        primary: true,
                    }
                    .on_press(move || retry_core.invoke_or_log(Command::ExtensionInstallRetry { install_id }))
                    .into_element(),
                );
            }
        }
        ExtensionInstallStage::Installed { .. } => {
            let manage_core = core.clone();
            buttons.push(
                DialogButton {
                    label: "Manage".to_string(),
                    primary: false,
                }
                .on_press(move || {
                    manage_core.invoke_or_log(Command::ExtensionInstallDismiss { install_id });
                    manage_core.invoke_or_log(Command::SettingsOpen {
                        group: Some(SettingsGroupId::Extensions),
                    });
                })
                .into_element(),
            );
            if let Some(info) = installed_info.filter(|info| !info.pinned_to_toolbar) {
                let pin_core = core.clone();
                let extension_id = info.id.clone();
                buttons.push(
                    DialogButton {
                        label: "Pin to toolbar".to_string(),
                        primary: true,
                    }
                    .on_press(move || {
                        pin_core.invoke_or_log(Command::ExtensionSetToolbarPinned {
                            extension_id: extension_id.clone(),
                            pinned: true,
                        });
                        pin_core.invoke_or_log(Command::ExtensionInstallDismiss { install_id });
                    })
                    .into_element(),
                );
            }
        }
        _ => {}
    }
    buttons
}

/// The check (or warning) that pops onto the icon when an install ends.
#[derive(Clone, PartialEq)]
struct StageBadge {
    failed: bool,
    key: u64,
}

impl Component for StageBadge {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let pop = use_animation_with_dependencies(&(self.failed, self.key), |conf, _| {
            conf.on_creation(OnCreation::Run);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(motion::TOAST_POP_IN)
                .ease(Ease::Out)
                .function(motion::spring_fn())
        });
        let t = pop.get().value();
        let role = if self.failed {
            ThemeRole::Danger
        } else {
            ThemeRole::Success
        };
        let fg = if theme.scheme.is_dark() {
            theme.color(ThemeRole::Surface)
        } else {
            Color::WHITE
        };
        rect()
            .width(Size::px(BADGE_SIZE))
            .height(Size::px(BADGE_SIZE))
            .corner_radius(CornerRadius::new_all(theme.radii.pill))
            .background(theme.color(role))
            .border(
                Border::new()
                    .width(2.0)
                    .alignment(BorderAlignment::Outer)
                    .fill(theme.color(ThemeRole::Surface)),
            )
            .scale(t.max(0.0))
            .opacity(t.clamp(0.0, 1.0))
            .center()
            .child(
                label()
                    .text(if self.failed { "!" } else { "✓" })
                    .font_size(11.0)
                    .font_weight(FontWeight::BOLD)
                    .color(fg),
            )
    }
}

fn stage_index(stage: &ExtensionInstallStage) -> u64 {
    match stage {
        ExtensionInstallStage::Downloading => 1,
        ExtensionInstallStage::Checking => 2,
        ExtensionInstallStage::Installing => 3,
        ExtensionInstallStage::Installed { .. } => 4,
        ExtensionInstallStage::Failed { .. } => 5,
    }
}

fn display_name(install: &ExtensionInstall) -> String {
    if install.name.trim().is_empty() {
        "the extension".to_string()
    } else {
        install.name.clone()
    }
}

/// Title, subtitle, and whether to show the progress line.
fn describe(install: &ExtensionInstall, name: &str) -> (String, String, bool) {
    let from = if install.source.is_empty() {
        String::new()
    } else {
        format!(" from {}", install.source)
    };
    match &install.stage {
        ExtensionInstallStage::Downloading => (format!("Adding {name}"), format!("Downloading{from}…"), true),
        ExtensionInstallStage::Checking => (format!("Adding {name}"), "Checking the package…".to_string(), true),
        ExtensionInstallStage::Installing => (format!("Adding {name}"), "Installing…".to_string(), true),
        ExtensionInstallStage::Installed { updated } => (
            if *updated {
                format!("{name} was updated")
            } else {
                format!("{name} was added")
            },
            "Open it from the extensions menu, or pin it to the toolbar.".to_string(),
            false,
        ),
        ExtensionInstallStage::Failed { reason, .. } => (
            format!("Couldn't add {name}"),
            if reason.is_empty() {
                "Something went wrong.".to_string()
            } else {
                reason.clone()
            },
            false,
        ),
    }
}

fn schedule_dismiss(mut timer_gen: State<u64>, hovered: State<bool>, core: CoreView, install_id: u64) {
    let next_gen = timer_gen.peek().wrapping_add(1);
    timer_gen.set(next_gen);
    spawn(async move {
        timer(INSTALLED_CARD_TIMEOUT).await;
        let still_shown = core
            .model()
            .peek()
            .extension_install
            .as_ref()
            .is_some_and(|i| i.install_id == install_id);
        if *timer_gen.peek() == next_gen && !*hovered.peek() && still_shown {
            core.invoke_or_log(Command::ExtensionInstallDismiss { install_id });
        }
    });
}
