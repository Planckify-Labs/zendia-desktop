/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Component Kit gallery section showcasing every shared component in every visual state.

use crate::gallery::sections::{demo_card, glass_backdrop, state_caption};
use crate::ui::core_view::use_core;
use crate::ui::kit::{
    AnchoredPopover, ConfirmationCard, FadeTransition, GlassPanel, Icon, IconButton, KeyLabel, KitCheckbox, KitChip,
    KitSlider, KitSwitch, ListRow, Pill, SlideDirection, SlideTransition, Toast,
};
use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_module_api::ui::{UiPlacement, UiTone};

#[derive(Clone, PartialEq)]
pub struct KitSection {
    pub state_id: String,
}

fn surfaces_cards() -> Element {
    let theme = use_theme();
    let core = use_core();

    let pill_core = core.clone();
    let toast_core = core.clone();

    rect()
        .width(Size::fill())
        .spacing(theme.spacing.xxl)
        .child(demo_card(
            "GlassPanel & Pill",
            "Translucent glass container over content, solid fallback, and Pills across tones, dots, and icons.",
            rect()
                .spacing(theme.spacing.xl)
                .child(glass_backdrop(
                    rect()
                        .width(Size::fill())
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .spacing(theme.spacing.xl)
                        .child(
                            GlassPanel::new()
                                .width(Size::flex(1.0))
                                .glass(true)
                                .child(label().text("Floating Glass Panel").font_weight(FontWeight::SEMI_BOLD))
                                .child(
                                    label()
                                        .text("Backdrop blur + hairline outline + ambient drop shadow.")
                                        .font_size(theme.font_sizes.sm)
                                        .color(theme.color(ThemeRole::TextMuted)),
                                ),
                        )
                        .child(
                            GlassPanel::new()
                                .width(Size::flex(1.0))
                                .glass(false)
                                .child(label().text("Solid Surface Panel").font_weight(FontWeight::SEMI_BOLD))
                                .child(
                                    label()
                                        .text("Opaque fallback when glass translucency is disabled.")
                                        .font_size(theme.font_sizes.sm)
                                        .color(theme.color(ThemeRole::TextMuted)),
                                ),
                        ),
                ))
                .child(state_caption("PILLS (TONES, DOTS, ICONS & INTERACTIVE)"))
                .child(
                    rect()
                        .direction(Direction::Horizontal)
                        .content(Content::wrap())
                        .spacing(theme.spacing.md)
                        .child(Pill::new("Neutral").dot(ThemeRole::ModeNormal))
                        .child(
                            Pill::new("Agent working")
                                .icon("chat")
                                .tone(UiTone::Accent)
                                .on_press(move |_| {
                                    pill_core.invoke_or_log(Command::Custom {
                                        name: "gallery.pill.clicked".to_string(),
                                        args: vec![("tone".to_string(), "accent".to_string())],
                                    });
                                }),
                        )
                        .child(Pill::new("Connected").icon("check").tone(UiTone::Success))
                        .child(Pill::new("Waiting on user").icon("warning").tone(UiTone::Warning))
                        .child(
                            Pill::new("Service stopped")
                                .icon("security-warning")
                                .tone(UiTone::Danger),
                        ),
                ),
        ))
        .child(demo_card(
            "Toast",
            "Short notification banners across neutral, accent, success, warning, and danger tones.",
            rect()
                .width(Size::fill())
                .spacing(theme.spacing.md)
                .child(
                    Toast::new("Copied page URL")
                        .detail("https://ladybird.org/")
                        .tone(UiTone::Neutral)
                        .on_dismiss({
                            let core = toast_core.clone();
                            move |_| {
                                core.invoke_or_log(Command::Custom {
                                    name: "gallery.toast.dismiss".to_string(),
                                    args: vec![("tone".to_string(), "neutral".to_string())],
                                });
                            }
                        }),
                )
                .child(
                    Toast::new("Switched to Research space")
                        .detail("4 tabs restored")
                        .tone(UiTone::Accent),
                )
                .child(
                    Toast::new("Download completed")
                        .detail("zendia-spec.pdf (2.4 MB)")
                        .tone(UiTone::Success),
                )
                .child(
                    Toast::new("AgentServer restarting")
                        .detail("Run trace preserved")
                        .tone(UiTone::Warning),
                )
                .child(
                    Toast::new("Extension host unavailable")
                        .detail("Extensions paused until restart")
                        .tone(UiTone::Danger)
                        .on_dismiss(move |_| {
                            toast_core.invoke_or_log(Command::Custom {
                                name: "gallery.toast.dismiss".to_string(),
                                args: vec![("tone".to_string(), "danger".to_string())],
                            });
                        }),
                ),
        ))
        .into_element()
}

fn controls_card() -> Element {
    let theme = use_theme();
    let core = use_core();

    let mut chip_selected = use_state(|| true);
    let mut checkbox_off = use_state(|| false);
    let mut checkbox_on = use_state(|| true);
    let mut switch_on = use_state(|| true);
    let mut switch_off = use_state(|| false);
    let mut continuous_val = use_state(|| 64.0_f32);
    let mut effort_val = use_state(|| 2.0_f32);

    let effort_labels = ["None", "Low", "Medium", "High", "Max"];
    let effort_idx = (effort_val().round() as usize).min(effort_labels.len() - 1);

    let icon_core = core.clone();
    let chip_core = core.clone();
    let checkbox_core = core.clone();
    let switch_core = core.clone();
    let slider_core = core.clone();

    demo_card(
        "IconButton, KitChip, KitCheckbox, KitSwitch & KitSlider",
        "Interactive controls across normal, hover, active, selected, and disabled states.",
        rect()
            .spacing(theme.spacing.xl)
            .child(state_caption(
                "ICON BUTTON (NORMAL · HOVER PREVIEW · ACTIVE · DISABLED)",
            ))
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(theme.spacing.xl)
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(theme.spacing.sm)
                            .child(IconButton::new("reload", "Normal reload").on_press({
                                let core = icon_core.clone();
                                move |_| {
                                    core.invoke_or_log(Command::NavigationReload {
                                        tab_id: None,
                                        bypass_cache: false,
                                    });
                                }
                            }))
                            .child(
                                label()
                                    .text("Normal")
                                    .font_size(theme.font_sizes.xs)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            ),
                    )
                    .child(
                        // Static hover-appearance preview alongside the live interactive hover state.
                        rect()
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(theme.spacing.sm)
                            .child(
                                rect()
                                    .width(Size::px(28.0))
                                    .height(Size::px(28.0))
                                    .main_align(Alignment::center())
                                    .cross_align(Alignment::center())
                                    .corner_radius(CornerRadius::new_all(theme.radii.sm))
                                    .background(theme.color(ThemeRole::Hover))
                                    .child(Icon::new("search").size(14.0).role(ThemeRole::Text)),
                            )
                            .child(
                                label()
                                    .text("Hover")
                                    .font_size(theme.font_sizes.xs)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            ),
                    )
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(theme.spacing.sm)
                            .child(IconButton::new("chat", "Active chat").active(true).on_press(move |_| {
                                icon_core.invoke_or_log(Command::Custom {
                                    name: "gallery.icon_button.active".to_string(),
                                    args: Vec::new(),
                                });
                            }))
                            .child(
                                label()
                                    .text("Active")
                                    .font_size(theme.font_sizes.xs)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            ),
                    )
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(theme.spacing.sm)
                            .child(IconButton::new("forward", "Disabled forward").disabled(true))
                            .child(
                                label()
                                    .text("Disabled")
                                    .font_size(theme.font_sizes.xs)
                                    .color(theme.color(ThemeRole::TextMuted)),
                            ),
                    ),
            )
            .child(state_caption("CHIPS (UNSELECTED · SELECTED · TONES · DISABLED)"))
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .spacing(theme.spacing.md)
                    .child(
                        KitChip::new("Toggleable")
                            .icon("sparkles")
                            .selected(chip_selected())
                            .on_press(move |_| {
                                let next = !*chip_selected.peek();
                                chip_selected.set(next);
                                chip_core.invoke_or_log(Command::Custom {
                                    name: "gallery.chip.toggle".to_string(),
                                    args: vec![("selected".to_string(), next.to_string())],
                                });
                            }),
                    )
                    .child(KitChip::new("Neutral"))
                    .child(KitChip::new("Accent").icon("chat").tone(UiTone::Accent).selected(true))
                    .child(KitChip::new("Ready").icon("check").tone(UiTone::Success).selected(true))
                    .child(
                        KitChip::new("Pending")
                            .icon("warning")
                            .tone(UiTone::Warning)
                            .selected(true),
                    )
                    .child(
                        KitChip::new("Blocked")
                            .icon("block")
                            .tone(UiTone::Danger)
                            .selected(true),
                    )
                    .child(KitChip::new("Disabled").disabled(true)),
            )
            .child(state_caption("CHECKBOXES (UNCHECKED · CHECKED · DISABLED)"))
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .spacing(theme.spacing.xxl)
                    .child(KitCheckbox::new(checkbox_off()).label("Unchecked").on_toggle({
                        let core = checkbox_core.clone();
                        move |next| {
                            checkbox_off.set(next);
                            core.invoke_or_log(Command::Custom {
                                name: "gallery.checkbox.off".to_string(),
                                args: vec![("value".to_string(), next.to_string())],
                            });
                        }
                    }))
                    .child(
                        KitCheckbox::new(checkbox_on())
                            .label("Restore pinned tabs")
                            .on_toggle(move |next| {
                                checkbox_on.set(next);
                                checkbox_core.invoke_or_log(Command::Custom {
                                    name: "gallery.checkbox.checked".to_string(),
                                    args: vec![("value".to_string(), next.to_string())],
                                });
                            }),
                    )
                    .child(KitCheckbox::new(false).label("Disabled off").disabled(true))
                    .child(KitCheckbox::new(true).label("Disabled on").disabled(true)),
            )
            .child(state_caption("SWITCHES (OFF · ON · DISABLED)"))
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .content(Content::wrap())
                    .spacing(theme.spacing.xxl)
                    .child(KitSwitch::new(switch_off()).label("Off").on_toggle({
                        let core = switch_core.clone();
                        move |next| {
                            switch_off.set(next);
                            core.invoke_or_log(Command::Custom {
                                name: "gallery.switch.off".to_string(),
                                args: vec![("value".to_string(), next.to_string())],
                            });
                        }
                    }))
                    .child(KitSwitch::new(switch_on()).label("Vim mode").on_toggle(move |next| {
                        switch_on.set(next);
                        switch_core.invoke_or_log(Command::Custom {
                            name: "gallery.switch.vim".to_string(),
                            args: vec![("value".to_string(), next.to_string())],
                        });
                    }))
                    .child(KitSwitch::new(false).label("Disabled off").disabled(true))
                    .child(KitSwitch::new(true).label("Disabled on").disabled(true)),
            )
            .child(state_caption("SLIDERS (CONTINUOUS · STEPPED · DISABLED)"))
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .spacing(theme.spacing.xxl)
                    .child(
                        rect().width(Size::flex(1.0)).child(
                            KitSlider::new(continuous_val())
                                .label(format!("Glass opacity ({:.0}%)", continuous_val()))
                                .range(0.0, 100.0)
                                .on_change({
                                    let core = slider_core.clone();
                                    move |next| {
                                        continuous_val.set(next);
                                        core.invoke_or_log(Command::Custom {
                                            name: "gallery.slider.opacity".to_string(),
                                            args: vec![("value".to_string(), format!("{next:.0}"))],
                                        });
                                    }
                                }),
                        ),
                    )
                    .child(
                        rect().width(Size::flex(1.0)).child(
                            KitSlider::new(effort_val())
                                .label(format!("Reasoning effort ({})", effort_labels[effort_idx]))
                                .range(0.0, 4.0)
                                .step(Some(1.0))
                                .accent_role(ThemeRole::EffortHigh)
                                .on_change(move |next| {
                                    effort_val.set(next);
                                    slider_core.invoke_or_log(Command::Custom {
                                        name: "gallery.slider.effort".to_string(),
                                        args: vec![("level".to_string(), format!("{next:.0}"))],
                                    });
                                }),
                        ),
                    )
                    .child(
                        rect().width(Size::flex(1.0)).child(
                            KitSlider::new(35.0)
                                .label("Disabled slider (35%)")
                                .range(0.0, 100.0)
                                .disabled(true),
                        ),
                    ),
            ),
    )
}

fn lists_and_prompts_cards() -> Element {
    let theme = use_theme();
    let core = use_core();
    let mut cursor_index = use_state(|| 1_usize);
    let mut selected_index = use_state(|| 2_usize);

    let confirm_core = core.clone();
    let danger_core = core.clone();
    let row_core = core.clone();

    let sample_rows: &[(&str, &str, &str, &str, bool)] = &[
        ("ladybird.org", "Standard row", "globe", "<C-1>", false),
        ("Architecture.md", "Active cursor row", "book", "<C-j>", false),
        ("Zendia Agent Chat", "Selected row", "chat", "<leader>a", false),
        ("Closed archive tab", "Disabled row", "folder", "<C-4>", true),
    ];

    rect()
        .width(Size::fill())
        .spacing(theme.spacing.xxl)
        .child(demo_card(
            "KeyLabel & ListRow",
            "Vim and standard key-cap notation plus ListRow in normal, cursor, selected, and disabled states.",
            rect()
                .spacing(theme.spacing.xl)
                .child(state_caption("KEY LABELS (INACTIVE & ACTIVE HINT STRENGTH)"))
                .child(
                    rect()
                        .direction(Direction::Horizontal)
                        .content(Content::wrap())
                        .cross_align(Alignment::center())
                        .spacing(theme.spacing.lg)
                        .child(KeyLabel::new("<C-j>"))
                        .child(KeyLabel::new("<C-S-Tab>"))
                        .child(KeyLabel::new("<A-Left>"))
                        .child(KeyLabel::new("<leader>ff"))
                        .child(KeyLabel::new("Ctrl+T"))
                        .child(KeyLabel::new("<CR>").active(true))
                        .child(KeyLabel::new("<C-A-m>").active(true)),
                )
                .child(state_caption("LIST ROWS (CLICK TO MOVE CURSOR)"))
                .child(
                    rect()
                        .width(Size::fill())
                        .spacing(theme.spacing.xs)
                        .children(sample_rows.iter().enumerate().map(
                            |(idx, (title, detail, icon, hint, disabled))| {
                                let core = row_core.clone();
                                let title_owned = (*title).to_string();
                                let mut row = ListRow::new(*title)
                                    .key(idx)
                                    .detail(*detail)
                                    .icon(*icon)
                                    .key_hint(*hint)
                                    .cursor(cursor_index() == idx)
                                    .selected(selected_index() == idx)
                                    .disabled(*disabled);
                                if !disabled {
                                    row = row.on_press(move |_| {
                                        cursor_index.set(idx);
                                        if idx == 2 {
                                            selected_index.set(idx);
                                        }
                                        core.invoke_or_log(Command::Custom {
                                            name: "gallery.list_row.press".to_string(),
                                            args: vec![("row".to_string(), title_owned.clone())],
                                        });
                                    });
                                }
                                row
                            },
                        )),
                ),
        ))
        .child(demo_card(
            "ConfirmationCard (Normal & Danger)",
            "Inline registry confirmation prompt shared by agents, plugins, and extensions.",
            rect()
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .content(Content::Flex)
                .spacing(theme.spacing.xl)
                .child(
                    rect().width(Size::flex(1.0)).child(
                        ConfirmationCard::new(
                            "Allow opening 3 research tabs?",
                            "The agent wants to open three documentation pages in the Research space.",
                        )
                        .confirm_label("Allow")
                        .cancel_label("Deny")
                        .danger(false)
                        .on_confirm({
                            let core = confirm_core.clone();
                            move |_| {
                                core.invoke_or_log(Command::Custom {
                                    name: "agent.confirm".to_string(),
                                    args: vec![("allow".to_string(), "true".to_string())],
                                });
                            }
                        })
                        .on_cancel(move |_| {
                            confirm_core.invoke_or_log(Command::Custom {
                                name: "agent.confirm".to_string(),
                                args: vec![("allow".to_string(), "false".to_string())],
                            });
                        }),
                    ),
                )
                .child(
                    rect().width(Size::flex(1.0)).child(
                        ConfirmationCard::new(
                            "Clear site cookies and storage?",
                            "This signs you out of example.com and removes all local storage data.",
                        )
                        .confirm_label("Clear Data")
                        .cancel_label("Keep")
                        .danger(true)
                        .on_confirm({
                            let core = danger_core.clone();
                            move |_| {
                                core.invoke_or_log(Command::Custom {
                                    name: "site.clear_data".to_string(),
                                    args: vec![("confirm".to_string(), "true".to_string())],
                                });
                            }
                        })
                        .on_cancel(move |_| {
                            danger_core.invoke_or_log(Command::Custom {
                                name: "site.clear_data".to_string(),
                                args: vec![("confirm".to_string(), "false".to_string())],
                            });
                        }),
                    ),
                ),
        ))
        .into_element()
}

#[derive(Clone, PartialEq)]
struct OverlaysAndMotionCard {
    open_popover_initially: bool,
}

impl Component for OverlaysAndMotionCard {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let core = use_core();

        let mut popover_open = use_state(|| self.open_popover_initially);
        let mut placement = use_state(|| UiPlacement::Bottom);
        let mut motion_visible = use_state(|| true);
        let mut slide_dir = use_state(|| SlideDirection::Up);
        let mut replay_count = use_state(|| 0_u64);

        let popover_core = core.clone();

        let anchor_button = KitChip::new(if popover_open() {
            "Close Anchored Popover"
        } else {
            "Open Anchored Popover"
        })
        .icon("popup")
        .selected(popover_open())
        .on_press(move |_| {
            let next = !*popover_open.peek();
            popover_open.set(next);
        });

        let placements: &[(&str, UiPlacement)] = &[
            ("Top", UiPlacement::Top),
            ("Bottom", UiPlacement::Bottom),
            ("Left", UiPlacement::Left),
            ("Right", UiPlacement::Right),
        ];

        let directions: &[(&str, SlideDirection)] = &[
            ("Up", SlideDirection::Up),
            ("Down", SlideDirection::Down),
            ("Left", SlideDirection::Left),
            ("Right", SlideDirection::Right),
        ];

        rect()
            .width(Size::fill())
            .spacing(theme.spacing.xxl)
            .child(demo_card(
                "AnchoredPopover (via Portal)",
                "Lifted to Layer::Overlay and anchored to its trigger; Esc or outside click dismisses.",
                rect()
                    .spacing(theme.spacing.lg)
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .cross_align(Alignment::center())
                            .spacing(theme.spacing.md)
                            .child(state_caption("PLACEMENT:"))
                            .children(placements.iter().map(|(name, value)| {
                                let value = *value;
                                KitChip::new(*name)
                                    .key(*name)
                                    .selected(placement() == value)
                                    .on_press(move |_| placement.set(value))
                            })),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::px(110.0))
                            .main_align(Alignment::center())
                            .cross_align(Alignment::center())
                            .child(
                                AnchoredPopover::new("gallery-kit-popover", anchor_button)
                                    .open(popover_open())
                                    .placement(placement())
                                    .on_dismiss(move |_| popover_open.set(false))
                                    .child(
                                        rect()
                                            .width(Size::px(260.0))
                                            .spacing(theme.spacing.sm)
                                            .child(
                                                label()
                                                    .text("Portal Popover")
                                                    .font_size(theme.font_sizes.md)
                                                    .font_weight(FontWeight::SEMI_BOLD),
                                            )
                                            .child(
                                                label()
                                                    .text("Rendered above every sibling via Freya Portal.")
                                                    .font_size(theme.font_sizes.xs)
                                                    .color(theme.color(ThemeRole::TextMuted)),
                                            )
                                            .child(
                                                ListRow::new("Invoke sample action")
                                                    .icon("bolt")
                                                    .key_hint("<CR>")
                                                    .on_press(move |_| {
                                                        popover_open.set(false);
                                                        popover_core.invoke_or_log(Command::Custom {
                                                            name: "gallery.popover.action".to_string(),
                                                            args: Vec::new(),
                                                        });
                                                    }),
                                            ),
                                    ),
                            ),
                    ),
            ))
            .child(demo_card(
                "Fade & Slide Transitions",
                "Zendia's fork motion curves (spring + cubic-bezier) with Replay and visibility toggle.",
                rect()
                    .spacing(theme.spacing.lg)
                    .child(
                        rect()
                            .direction(Direction::Horizontal)
                            .content(Content::wrap())
                            .cross_align(Alignment::center())
                            .spacing(theme.spacing.md)
                            .child(
                                KitChip::new("Replay")
                                    .icon("reload")
                                    .tone(UiTone::Accent)
                                    .selected(true)
                                    .on_press(move |_| {
                                        let next_count = *replay_count.peek() + 1;
                                        replay_count.set(next_count);
                                        motion_visible.set(false);
                                        let (tx, rx) = async_channel::bounded::<()>(1);
                                        std::thread::spawn(move || {
                                            std::thread::sleep(Duration::from_millis(160));
                                            let _ = tx.send_blocking(());
                                        });
                                        spawn_in_window(async move {
                                            let _ = rx.recv().await;
                                            motion_visible.set(true);
                                        });
                                    }),
                            )
                            .child(
                                KitChip::new(if motion_visible() { "Hide" } else { "Show" })
                                    .icon("eye")
                                    .on_press(move |_| {
                                        let next = !*motion_visible.peek();
                                        motion_visible.set(next);
                                    }),
                            )
                            .children(directions.iter().map(|(name, dir)| {
                                let dir = *dir;
                                KitChip::new(*name)
                                    .key(*name)
                                    .selected(slide_dir() == dir)
                                    .on_press(move |_| slide_dir.set(dir))
                            })),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .min_height(Size::px(96.0))
                            .direction(Direction::Horizontal)
                            .content(Content::Flex)
                            .spacing(theme.spacing.xl)
                            .child(
                                rect()
                                    .width(Size::flex(1.0))
                                    .spacing(theme.spacing.sm)
                                    .child(state_caption(&format!("FADE TRANSITION (REPLAYS: {})", replay_count())))
                                    .child(
                                        FadeTransition::new(motion_visible()).child(
                                            GlassPanel::new()
                                                .width(Size::fill())
                                                .glass(false)
                                                .child(
                                                    label().text("FadeTransition").font_weight(FontWeight::SEMI_BOLD),
                                                )
                                                .child(
                                                    label()
                                                        .text("200 ms ease-out enter · 110 ms ease-in exit")
                                                        .font_size(theme.font_sizes.xs)
                                                        .color(theme.color(ThemeRole::TextMuted)),
                                                ),
                                        ),
                                    ),
                            )
                            .child(
                                rect()
                                    .width(Size::flex(1.0))
                                    .spacing(theme.spacing.sm)
                                    .child(state_caption(&format!("SLIDE TRANSITION ({:?})", slide_dir())))
                                    .child(
                                        SlideTransition::new(motion_visible(), slide_dir()).child(
                                            GlassPanel::new()
                                                .width(Size::fill())
                                                .glass(false)
                                                .child(
                                                    label().text("SlideTransition").font_weight(FontWeight::SEMI_BOLD),
                                                )
                                                .child(
                                                    label()
                                                        .text("220 ms Zendia spring · 14 px travel")
                                                        .font_size(theme.font_sizes.xs)
                                                        .color(theme.color(ThemeRole::TextMuted)),
                                                ),
                                        ),
                                    ),
                            ),
                    ),
            ))
    }
}

impl Component for KitSection {
    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.state_id)
    }

    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let show_surfaces = self.state_id == "all" || self.state_id == "surfaces";
        let show_controls = self.state_id == "all" || self.state_id == "controls";
        let show_lists = self.state_id == "all" || self.state_id == "lists";
        let show_overlays = self.state_id == "all" || self.state_id == "overlays";
        let open_popover = self.state_id == "overlays";

        ScrollView::new().width(Size::fill()).height(Size::fill()).child(
            rect()
                .width(Size::fill())
                .padding(Gaps::new_all(theme.spacing.xxl))
                .spacing(theme.spacing.xxl)
                .maybe_child(show_surfaces.then(surfaces_cards))
                .maybe_child(show_controls.then(controls_card))
                .maybe_child(show_lists.then(lists_and_prompts_cards))
                .maybe_child(show_overlays.then_some(OverlaysAndMotionCard {
                    open_popover_initially: open_popover,
                })),
        )
    }
}
