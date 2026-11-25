/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Which-key "helix" popup and all-keymaps view (`<leader>?`), floated at [`POPUP_OVERLAY_LEVEL`].
//!
//! Displays the breadcrumb title, right-aligned orange keys, muted `➜` separator, descriptions (with `+group` entries
//! tinted in their configured group color), and the `ESC close · BS back` footer. Reports its measured width to
//! [`VimStatuslineState`] so the mascot smoothly slides beside it.

use crate::ui::core_view::use_core;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, ResolvedTheme, Rgba, ThemeRole, motion, use_theme};
use crate::ui::vim::statusline::{STATUSLINE_CORNER_GAP, try_use_vim_statusline_state};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::input_state::{self, WhichKeyItem};
use zendia_browser_core::model::{InputMode, SidebarSide, WhichKeyState};

/// Popup corner radius matching `--zendia-float-radius`.
const WHICH_KEY_RADIUS: f32 = 18.0;
/// Minimum popup width (~30ch).
const WHICH_KEY_MIN_WIDTH: f32 = 248.0;
/// Maximum popup width (~60ch).
const WHICH_KEY_MAX_WIDTH: f32 = 430.0;
/// Maximum scrollable list height so long tables like All Keymaps stay within the viewport.
const WHICH_KEY_MAX_LIST_HEIGHT: f32 = 540.0;
/// Horizontal slide transition when moving into or out of a subgroup.
const SUBGROUP_SLIDE_DURATION: Duration = Duration::from_millis(180);

/// Resolves a group color name from `vim.toml` onto the active theme's Tokyonight palette.
pub fn resolve_group_color(theme: &ResolvedTheme, color_name: Option<&str>) -> Color {
    match color_name {
        Some("pink") => theme.palette.pink.to_color(),
        Some("red") => theme.palette.red.to_color(),
        Some("orange") => theme.palette.orange.to_color(),
        Some("yellow") => theme.palette.yellow.to_color(),
        Some("green") => theme.palette.green.to_color(),
        Some("teal") => theme.palette.teal.to_color(),
        Some("cyan") => theme.palette.cyan.to_color(),
        Some("blue") => theme.palette.blue.to_color(),
        Some("purple") => theme.palette.purple.to_color(),
        _ => theme.color(ThemeRole::WhichKeyGroup),
    }
}

/// Converts either the view-level all-keymaps popup or the model's `WhichKeyState` into display rows.
fn resolve_popup_rows(
    all_keymaps_open: bool,
    all_keymaps_mode: InputMode,
    mode: InputMode,
    pending_keys: &[String],
    model_which_key: Option<&WhichKeyState>,
) -> Option<(String, Vec<WhichKeyItem>)> {
    if all_keymaps_open {
        let view = input_state::which_key_all_keymaps(all_keymaps_mode);
        return Some((view.title, view.items));
    }
    let wk = model_which_key?;
    if wk.entries.is_empty() {
        return None;
    }
    let items = wk
        .entries
        .iter()
        .map(|entry| {
            let clean_desc = entry.description.strip_prefix('+').unwrap_or(&entry.description);
            let group_color = if entry.is_group {
                input_state::group_color_for_entry(mode, pending_keys, &entry.key)
            } else {
                None
            };
            WhichKeyItem {
                key: entry.key.clone(),
                label: entry.key.clone(),
                description: clean_desc.to_string(),
                is_group: entry.is_group,
                group_color,
            }
        })
        .collect();
    Some((wk.title.clone(), items))
}

/// Estimates a clean right-aligned key column width in logical pixels based on the longest key label.
fn key_column_width(items: &[WhichKeyItem]) -> f32 {
    let max_chars = items.iter().map(|item| item.label.chars().count()).max().unwrap_or(1);
    ((max_chars as f32 * 7.6) + 8.0).clamp(26.0, 96.0)
}

/// Which-key "helix" floating popup for pending key sequences and `<leader>?` all-keymaps view.
#[derive(Clone, PartialEq)]
pub struct WhichKeyPopup {
    /// Disables entrance animations for deterministic gallery captures.
    pub static_preview: bool,
    pub key: DiffKey,
}

impl Default for WhichKeyPopup {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyExt for WhichKeyPopup {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl WhichKeyPopup {
    pub fn new() -> Self {
        Self {
            static_preview: false,
            key: DiffKey::None,
        }
    }

    pub fn static_preview(mut self, static_preview: bool) -> Self {
        self.static_preview = static_preview;
        self
    }
}

impl Component for WhichKeyPopup {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let vim_state = try_use_vim_statusline_state();

        let mut last_title = use_state(String::new);
        let mut last_depth = use_state(|| 0usize);
        let mut slide_dir = use_state(|| 0.0f32);
        let mut slide_seq = use_state(|| 0u64);

        // Clear the reported width if this popup unmounts while open.
        use_drop(move || {
            if let Some(state) = vim_state {
                state.set_which_key_width(0.0);
            }
        });

        let (vim_enabled, mode, pending_keys, model_wk, sidebar_side) = {
            let model = core.model().read();
            (
                model.vim_mode_enabled,
                model.input_mode,
                model.pending_keys.clone(),
                model.which_key.clone(),
                model.layout.sidebar_side,
            )
        };

        let all_open = vim_state.is_some_and(|s| *s.all_keymaps_open.read());
        let all_mode = vim_state.map(|s| *s.all_keymaps_mode.read()).unwrap_or(mode);

        let payload = if vim_enabled {
            resolve_popup_rows(all_open, all_mode, mode, &pending_keys, model_wk.as_ref())
        } else {
            None
        };

        let is_open = payload.is_some();
        if !is_open && let Some(state) = vim_state {
            state.set_which_key_width(0.0);
        }

        let current_title = payload.as_ref().map(|(t, _)| t.clone()).unwrap_or_default();
        let current_depth = if all_open { 0 } else { pending_keys.len() };
        if is_open && *last_title.peek() != current_title {
            let prev_d = *last_depth.peek();
            let dir = if current_depth > prev_d {
                1.0
            } else if current_depth < prev_d {
                -1.0
            } else {
                0.0
            };
            last_title.set(current_title);
            last_depth.set(current_depth);
            slide_dir.set(dir);
            let next = *slide_seq.peek() + 1;
            slide_seq.set(next);
        }

        let open_anim = use_animation_with_dependencies(&is_open, move |conf, open| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *open { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::FLOAT_IN)
                .ease(Ease::Out)
                .function(motion::spring_fn())
        });

        let s_seq = *slide_seq.read();
        let list_slide_anim = use_animation_with_dependencies(&s_seq, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(SUBGROUP_SLIDE_DURATION)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let Some((title, items)) = payload else {
            return rect().width(Size::px(0.0)).height(Size::px(0.0));
        };

        let open_t = if self.static_preview {
            1.0
        } else {
            open_anim.get().value().clamp(0.0, 1.05)
        };
        let slide_t = if self.static_preview {
            1.0
        } else {
            list_slide_anim.get().value().clamp(0.0, 1.0)
        };
        let list_offset_x = *slide_dir.read() * 12.0 * (1.0 - slide_t);

        let key_col_w = key_column_width(&items);
        let max_desc_chars = items
            .iter()
            .map(|item| item.description.chars().count() + usize::from(item.is_group))
            .max()
            .unwrap_or(16);
        let panel_width =
            (key_col_w + 36.0 + max_desc_chars as f32 * 7.2 + 36.0).clamp(WHICH_KEY_MIN_WIDTH, WHICH_KEY_MAX_WIDTH);
        let key_color = theme.color(ThemeRole::WhichKeyKey);
        let text_color = theme.color(ThemeRole::Text);
        let muted_color = theme.color(ThemeRole::TextMuted);
        let sep_color = theme.rgba(ThemeRole::TextMuted).with_alpha(0.55).to_color();

        // Frosted high-contrast glass panel matching zendia.css while ensuring crisp legibility in both themes.
        let (glass_bg, border_color, shadow_color) = match theme.scheme {
            ResolvedScheme::Dark => (
                Rgba::from_rgb_alpha(26, 27, 38, 0.90).to_color(),
                Rgba::from_rgb_alpha(255, 255, 255, 0.14).to_color(),
                Color::from_argb(150, 0, 0, 0),
            ),
            ResolvedScheme::Light => (
                Rgba::from_rgb_alpha(250, 251, 255, 0.94).to_color(),
                Rgba::from_rgb_alpha(20, 24, 44, 0.14).to_color(),
                Color::from_argb(72, 20, 22, 40),
            ),
        };

        let rows: Vec<Element> = items
            .into_iter()
            .enumerate()
            .map(|(idx, item)| {
                let desc_text = if item.is_group {
                    format!("+{}", item.description)
                } else {
                    item.description
                };
                let desc_color = if item.is_group {
                    resolve_group_color(&theme, item.group_color.as_deref())
                } else {
                    text_color
                };
                rect()
                    .key(idx)
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
                    .padding(Gaps::new(1.5, 0.0, 1.5, 0.0))
                    .child(
                        rect()
                            .width(Size::px(key_col_w))
                            .main_align(Alignment::End)
                            .direction(Direction::Horizontal)
                            .child(
                                label()
                                    .text(item.label)
                                    .font_size(12.0)
                                    .font_weight(FontWeight::BOLD)
                                    .font_family("monospace")
                                    .max_lines(1)
                                    .color(key_color),
                            ),
                    )
                    .child(label().text("➜").font_size(11.0).max_lines(1).color(sep_color))
                    .child(
                        rect().width(Size::flex(1.0)).overflow(Overflow::Clip).child(
                            label()
                                .text(desc_text)
                                .font_size(13.0)
                                .font_weight(if item.is_group {
                                    FontWeight::MEDIUM
                                } else {
                                    FontWeight::NORMAL
                                })
                                .max_lines(1)
                                .text_overflow(TextOverflow::Ellipsis)
                                .color(desc_color),
                        ),
                    )
                    .into_element()
            })
            .collect();

        let estimated_rows_height = (rows.len() as f32 * 22.0 + 4.0).min(WHICH_KEY_MAX_LIST_HEIGHT);
        let needs_scroll = rows.len() as f32 * 22.0 > WHICH_KEY_MAX_LIST_HEIGHT;

        let list_body: Element = if needs_scroll {
            ScrollView::new()
                .width(Size::fill())
                .height(Size::px(estimated_rows_height))
                .child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                        .offset_x(list_offset_x)
                        .children(rows),
                )
                .into_element()
        } else {
            rect()
                .width(Size::fill())
                .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                .offset_x(list_offset_x)
                .children(rows)
                .into_element()
        };

        let on_right = sidebar_side == SidebarSide::Left;
        let position = if on_right {
            Position::new_absolute()
                .right(STATUSLINE_CORNER_GAP)
                .bottom(STATUSLINE_CORNER_GAP)
        } else {
            Position::new_absolute()
                .left(STATUSLINE_CORNER_GAP)
                .bottom(STATUSLINE_CORNER_GAP)
        };

        let dismiss_core = core.clone();
        let on_global_key_down = move |e: Event<KeyboardEventData>| {
            if e.key == Key::Named(NamedKey::Escape) {
                if let Some(state) = vim_state {
                    state.close_all_keymaps();
                }
                dismiss_core.invoke_or_log(Command::WhichKeyHide);
            }
        };

        let mut panel = rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(position)
            .min_width(Size::px(WHICH_KEY_MIN_WIDTH))
            .max_width(Size::px(WHICH_KEY_MAX_WIDTH))
            .width(Size::px(panel_width))
            .padding(Gaps::new_all(8.0))
            .spacing(2.0)
            .corner_radius(CornerRadius::new_all(WHICH_KEY_RADIUS))
            .background(glass_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_color),
            )
            .shadow(Shadow::new().y(14.0).blur(42.0).color(shadow_color))
            .overflow(Overflow::Clip)
            .opacity(open_t.clamp(0.0, 1.0))
            .offset_y(10.0 * (1.0 - open_t.clamp(0.0, 1.0)))
            .a11y_role(AccessibilityRole::Tooltip)
            .a11y_alt(format!("Which-key: {title}"))
            .on_global_key_down(on_global_key_down)
            .on_sized(move |e: Event<SizedEventData>| {
                if let Some(state) = vim_state {
                    state.set_which_key_width(e.area.width());
                }
            })
            .child(
                rect().padding(Gaps::new(4.0, 8.0, 6.0, 8.0)).child(
                    label()
                        .text(title)
                        .font_size(13.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis)
                        .color(muted_color),
                ),
            )
            .child(list_body)
            .child(
                rect().padding(Gaps::new(8.0, 8.0, 2.0, 8.0)).child(
                    label()
                        .text("ESC close · BS back")
                        .font_size(11.0)
                        .max_lines(1)
                        .color(muted_color),
                ),
            );

        if theme.glass.blur > 0.0 {
            panel = panel.backdrop_blur(theme.glass.blur);
        }
        panel
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
