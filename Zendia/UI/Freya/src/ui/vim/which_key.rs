/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Which-key "helix" popup and all-keymaps view (`<leader>?`), floated at [`POPUP_OVERLAY_LEVEL`].
//!
//! Displays the breadcrumb title, right-aligned orange keys, muted `➜` separator, descriptions (`+group` entries
//! colored with `ThemeRole::WhichKeyGroup`), and the `ESC close · BS back` footer. Matches the fork's frosted glass
//! material, `180ms` opacity / `300ms` spring entrance (`translateY(10px) scale(0.96)`), `110ms` exit fade/drop
//! (`translateY(6px) scale(0.98)`), `200ms` open delay for pending prefixes, subgroup horizontal slide + fade, and
//! `75vh` scrollable list with a thin accent scrollbar. Reports its measured width to [`VimStatuslineState`] so the
//! mascot smoothly slides beside it.

use crate::ui::core_view::use_core;
use crate::ui::kit::popup_presence::use_last_some;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, Rgba, ThemeRole, motion, use_theme};
use crate::ui::vim::statusline::{STATUSLINE_CORNER_GAP, try_use_vim_statusline_state};
use freya::animation::*;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::input_state::{self, WhichKeyItem};
use zendia_browser_core::model::{InputMode, SidebarSide, WhichKeyState};

/// Minimum popup width (~30ch).
const WHICH_KEY_MIN_WIDTH: f32 = 240.0;
/// Maximum popup width (~60ch), further capped to `90vw` of the stage.
const WHICH_KEY_MAX_WIDTH: f32 = 480.0;
/// Row height (`18px` line-height) plus `3px` row gap matching the fork's grid list.
const WHICH_KEY_ROW_HEIGHT: f32 = 18.0;
const WHICH_KEY_ROW_GAP: f32 = 3.0;
/// Approximate non-list vertical chrome inside the popup (7px*2 padding + title + footer + spacing).
const WHICH_KEY_CHROME_HEIGHT: f32 = 74.0;
/// Fallback stage dimensions before the first `on_sized` callback arrives.
const FALLBACK_STAGE_WIDTH: f32 = 1280.0;
const FALLBACK_STAGE_HEIGHT: f32 = 800.0;

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
    ((max_chars as f32 * 7.6) + 4.0).clamp(22.0, 96.0)
}

/// Computes the which-key entrance/exit transform `(opacity, offset_y, scale)` from the split opacity and transform
/// animation progress values.
pub fn which_key_transform(open: bool, opacity_t: f32, transform_t: f32) -> (f32, f32, f32) {
    let opacity = opacity_t.clamp(0.0, 1.0);
    if open {
        let offset_y = motion::WHICH_KEY_IN_Y * (1.0 - transform_t);
        let scale = motion::FLOAT_IN_SCALE + (1.0 - motion::FLOAT_IN_SCALE) * transform_t;
        (opacity, offset_y, scale)
    } else {
        let offset_y = motion::WHICH_KEY_OUT_Y * (1.0 - transform_t);
        let scale = motion::FLOAT_OUT_SCALE + (1.0 - motion::FLOAT_OUT_SCALE) * transform_t;
        (opacity, offset_y, scale)
    }
}

/// Computes `(min_width, max_width, max_list_height)` for a viewport/stage of `(stage_w, stage_h)` matching
/// `min-width: 30ch`, `max-width: min(60ch, 90vw)`, `max-height: 75vh`.
pub fn which_key_bounds(stage_w: f32, stage_h: f32) -> (f32, f32, f32) {
    let w = if stage_w > 1.0 { stage_w } else { FALLBACK_STAGE_WIDTH };
    let h = if stage_h > 1.0 { stage_h } else { FALLBACK_STAGE_HEIGHT };
    let max_w = (w * 0.90).clamp(WHICH_KEY_MIN_WIDTH, WHICH_KEY_MAX_WIDTH);
    let max_popup_h = (h * 0.75).max(200.0);
    let max_list_h = (max_popup_h - WHICH_KEY_CHROME_HEIGHT).max(120.0);
    (WHICH_KEY_MIN_WIDTH, max_w, max_list_h)
}

/// Returns true for standalone modifier keys that should not dismiss a pinned all-keymaps popup.
fn is_modifier_only_key(key: &Key) -> bool {
    matches!(
        key,
        Key::Named(
            NamedKey::Shift
                | NamedKey::Control
                | NamedKey::Alt
                | NamedKey::Meta
                | NamedKey::AltGraph
                | NamedKey::CapsLock
                | NamedKey::NumLock
                | NamedKey::ScrollLock
        )
    )
}

/// Which-key "helix" floating popup for pending key sequences and `<leader>?` all-keymaps view.
#[derive(Clone, PartialEq)]
pub struct WhichKeyPopup {
    /// Skips the initial `200ms` open delay for deterministic gallery captures.
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
        let (stage_w, stage_h) = vim_state
            .map(|s| *s.stage_size.read())
            .unwrap_or((FALLBACK_STAGE_WIDTH, FALLBACK_STAGE_HEIGHT));

        let raw_payload = if vim_enabled {
            resolve_popup_rows(all_open, all_mode, mode, &pending_keys, model_wk.as_ref())
        } else {
            None
        };
        let raw_open = raw_payload.is_some();

        // 200ms open delay when transitioning from hidden to a pending-prefix popup (showAll and static_preview open
        // immediately, and subgroup transitions while already visible update immediately).
        let mut delayed_visible = use_state(|| raw_open);
        let mut delay_seq = use_state(|| 0u64);
        let mut delay_armed = use_state(|| false);

        if !raw_open {
            if *delayed_visible.peek() {
                delayed_visible.set(false);
            }
            if *delay_armed.peek() {
                delay_armed.set(false);
                let next = *delay_seq.peek() + 1;
                delay_seq.set(next);
            }
        } else if !*delayed_visible.peek() {
            let show_immediately = self.static_preview || all_open || pending_keys.is_empty();
            if show_immediately {
                delay_armed.set_if_modified(false);
                delayed_visible.set(true);
            } else if !*delay_armed.peek() {
                delay_armed.set(true);
                let seq = *delay_seq.peek() + 1;
                delay_seq.set(seq);
                spawn(async move {
                    timer(motion::WHICH_KEY_DELAY).await;
                    if *delay_seq.peek() == seq && *delay_armed.peek() {
                        delay_armed.set(false);
                        delayed_visible.set(true);
                    }
                });
            }
        }

        let is_visible = raw_open && *delayed_visible.read();
        let visible_payload = if is_visible { raw_payload } else { None };
        let cached_payload = use_last_some(&visible_payload);

        let mut was_visible = use_state(|| is_visible);
        let mut last_title = use_state(|| visible_payload.as_ref().map(|(t, _)| t.clone()).unwrap_or_default());
        let mut last_depth = use_state(|| if all_open { 0 } else { pending_keys.len() });
        let mut slide_dir = use_state(|| 0.0f32);
        let mut slide_seq = use_state(|| 0u64);

        // Clear the reported width if this popup unmounts while open.
        use_drop(move || {
            if let Some(state) = vim_state {
                state.set_which_key_rect(0.0, 0.0);
            }
        });

        if !is_visible && let Some(state) = vim_state {
            state.set_which_key_rect(0.0, 0.0);
        }

        let current_title = visible_payload.as_ref().map(|(t, _)| t.clone()).unwrap_or_default();
        let current_depth = if all_open { 0 } else { pending_keys.len() };
        let prev_visible = *was_visible.peek();
        if is_visible {
            if !prev_visible {
                // Opening fresh: record title and depth without triggering a subgroup slide.
                was_visible.set(true);
                last_title.set(current_title);
                last_depth.set(current_depth);
                slide_dir.set_if_modified(0.0);
            } else if *last_title.peek() != current_title {
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
                if dir != 0.0 {
                    let next = *slide_seq.peek() + 1;
                    slide_seq.set(next);
                }
            }
        } else if prev_visible {
            was_visible.set(false);
            slide_dir.set_if_modified(0.0);
        }

        // Entrance: opacity 180ms ease-out + transform 300ms spring from translateY(10px) scale(0.96).
        // Exit: opacity + transform 110ms ease-in to translateY(6px) scale(0.98).
        let opacity_anim = use_animation_with_dependencies(&is_visible, move |conf, open| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            if *open {
                AnimNum::new(0.0, 1.0)
                    .duration(motion::WHICH_KEY_FADE_IN)
                    .ease(Ease::Out)
                    .function(motion::ease_out_fn())
            } else {
                AnimNum::new(1.0, 0.0)
                    .duration(motion::FLOAT_OUT)
                    .ease(Ease::In)
                    .function(motion::ease_in_fn())
            }
        });

        let transform_anim = use_animation_with_dependencies(&is_visible, move |conf, open| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            if *open {
                AnimNum::new(0.0, 1.0)
                    .duration(motion::FLOAT_IN)
                    .ease(Ease::Out)
                    .function(motion::spring_fn())
            } else {
                AnimNum::new(1.0, 0.0)
                    .duration(motion::FLOAT_OUT)
                    .ease(Ease::In)
                    .function(motion::ease_in_fn())
            }
        });

        let s_seq = *slide_seq.read();
        let list_slide_anim = use_animation_with_dependencies(&s_seq, move |conf, _| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            AnimNum::new(0.0, 1.0)
                .duration(motion::WHICH_KEY_SLIDE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let opacity_t = opacity_anim.get().value().clamp(0.0, 1.0);
        let transform_t = transform_anim.get().value().clamp(0.0, 1.05);
        let (opacity, offset_y, scale) = which_key_transform(is_visible, opacity_t, transform_t);

        if !is_visible && opacity <= 0.01 {
            return rect().width(Size::px(0.0)).height(Size::px(0.0));
        }

        let Some((title, items)) = cached_payload else {
            return rect().width(Size::px(0.0)).height(Size::px(0.0));
        };

        let active_dir = *slide_dir.read();
        let slide_t = list_slide_anim.get().value().clamp(0.0, 1.0);
        let list_offset_x = active_dir * 12.0 * (1.0 - slide_t);
        let list_opacity = if active_dir != 0.0 { slide_t } else { 1.0 };

        let (min_w, max_w, max_list_height) = which_key_bounds(stage_w, stage_h);
        let key_col_w = key_column_width(&items);
        let max_desc_chars = items
            .iter()
            .map(|item| item.description.chars().count() + usize::from(item.is_group))
            .max()
            .unwrap_or(16);
        let panel_width = (key_col_w + 32.0 + max_desc_chars as f32 * 7.2 + 34.0).clamp(min_w, max_w);

        let key_color = theme.color(ThemeRole::WhichKeyKey);
        let group_color = theme.color(ThemeRole::WhichKeyGroup);
        let text_color = theme.color(ThemeRole::Text);
        let muted_color = theme
            .rgba(ThemeRole::Text)
            .with_alpha(theme.glass.muted_opacity)
            .to_color();
        let sep_color = theme.rgba(ThemeRole::Text).with_alpha(0.50).to_color();

        let glass_bg = if theme.glass.float_blur > 0.0 {
            theme.glass.float_tint.to_color()
        } else {
            theme.glass.float_solid.to_color()
        };
        let border_color = theme.glass.outline.to_color();
        let inner_ring_color = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.05).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(255, 255, 255, 0.30).to_color(),
        };
        let sheen_gradient = LinearGradient::new()
            .angle(0.0)
            .stop((theme.glass.sheen_top.to_color(), 0.0))
            .stop((Color::from_argb(0, 255, 255, 255), 100.0));

        let scrollbar_base = theme.rgba(ThemeRole::Scrollbar);
        let sb_thumb = scrollbar_base.with_alpha(0.45).to_color();
        let sb_hover = scrollbar_base.with_alpha(0.75).to_color();
        let sb_active = scrollbar_base.with_alpha(0.90).to_color();

        let row_count = items.len();
        let rows: Vec<Element> = items
            .into_iter()
            .enumerate()
            .map(|(idx, item)| {
                let desc_text = if item.is_group {
                    format!("+{}", item.description)
                } else {
                    item.description
                };
                let desc_color = if item.is_group { group_color } else { text_color };
                rect()
                    .key(idx)
                    .width(Size::fill())
                    .min_height(Size::px(WHICH_KEY_ROW_HEIGHT))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(8.0)
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
                                .font_weight(FontWeight::NORMAL)
                                .max_lines(1)
                                .text_overflow(TextOverflow::Ellipsis)
                                .color(desc_color),
                        ),
                    )
                    .into_element()
            })
            .collect();

        let content_height = if row_count == 0 {
            0.0
        } else {
            row_count as f32 * WHICH_KEY_ROW_HEIGHT + (row_count - 1) as f32 * WHICH_KEY_ROW_GAP
        };
        let needs_scroll = content_height > max_list_height;

        let list_body: Element = if needs_scroll {
            ScrollView::new()
                .width(Size::fill())
                .height(Size::px(max_list_height))
                .auto_hide_scrollbars(false)
                .scrollbar(move |context: ScrollBarContext| {
                    ScrollBar::new(context)
                        .theme(ScrollBarThemePartial {
                            background: Some(Preference::Specific(Color::TRANSPARENT)),
                            thumb_background: Some(Preference::Specific(sb_thumb)),
                            hover_thumb_background: Some(Preference::Specific(sb_hover)),
                            active_thumb_background: Some(Preference::Specific(sb_active)),
                            thumb_cross_size: Some(Preference::Specific(3.5)),
                            expanded_thumb_cross_size: Some(Preference::Specific(5.5)),
                            opacity: Some(Preference::Specific(1.0)),
                            expanded_opacity: Some(Preference::Specific(1.0)),
                            cross_gap: Some(Preference::Specific(2.0)),
                            expanded_cross_gap: Some(Preference::Specific(2.0)),
                        })
                        .into_element()
                })
                .child(
                    rect()
                        .width(Size::fill())
                        .padding(Gaps::new(0.0, 12.0, 0.0, 8.0))
                        .spacing(WHICH_KEY_ROW_GAP)
                        .offset_x(list_offset_x)
                        .opacity(list_opacity)
                        .children(rows),
                )
                .into_element()
        } else {
            rect()
                .width(Size::fill())
                .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                .spacing(WHICH_KEY_ROW_GAP)
                .offset_x(list_offset_x)
                .opacity(list_opacity)
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
        let can_handle_keys = is_visible;
        let pinned_all = all_open;
        let on_global_key_down = move |e: Event<KeyboardEventData>| {
            if !can_handle_keys {
                return;
            }
            if pinned_all {
                if is_modifier_only_key(&e.key) {
                    return;
                }
                // Let Ctrl+Alt+M continue cycling debug states without eating it.
                let is_debug_cycle = e.modifiers.contains(Modifiers::CONTROL | Modifiers::ALT)
                    && matches!(&e.key, Key::Character(c) if c.eq_ignore_ascii_case("m"));
                if is_debug_cycle {
                    return;
                }
                if let Some(state) = vim_state {
                    state.close_all_keymaps();
                }
                dismiss_core.invoke_or_log(Command::WhichKeyHide);
            }
        };

        let report_width = is_visible;
        let mut panel = rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(position)
            .min_width(Size::px(min_w))
            .max_width(Size::px(max_w))
            .width(Size::px(panel_width))
            .padding(Gaps::new_all(7.0))
            .corner_radius(CornerRadius::new_all(theme.radii.float))
            .background(glass_bg)
            .border(
                Border::new()
                    .width(0.5)
                    .alignment(BorderAlignment::Inner)
                    .fill(border_color),
            )
            .shadow(theme.shadows.float_highlight.to_shadow())
            .shadow(Shadow::new().inset().spread(0.5).color(inner_ring_color))
            .shadow(theme.shadows.float_ambient.to_shadow())
            .shadow(theme.shadows.float_drop.to_shadow())
            .overflow(Overflow::Clip)
            .interactive(if is_visible { Interactive::Yes } else { Interactive::No })
            .opacity(opacity)
            .offset_y(offset_y)
            .scale(scale)
            .a11y_role(AccessibilityRole::Tooltip)
            .a11y_alt(format!("Which-key: {title}"))
            .on_global_key_down(on_global_key_down)
            .on_sized(move |e: Event<SizedEventData>| {
                if report_width && let Some(state) = vim_state {
                    state.set_which_key_rect(e.area.width(), e.area.height());
                }
            })
            .child(
                // Top specular sheen matching `--zendia-float-sheen`.
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .width(Size::percent(100.0))
                    .height(Size::px(96.0))
                    .background(sheen_gradient)
                    .interactive(Interactive::No),
            )
            .child(
                rect().padding(Gaps::new(4.0, 8.0, 8.0, 8.0)).child(
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

        let effective_blur = theme.glass.float_blur * opacity;
        if effective_blur >= 0.5 {
            panel = panel.backdrop_blur(effective_blur);
        }
        panel
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_key_transform_matches_enter_and_exit_endpoints() {
        let (op0, y0, s0) = which_key_transform(true, 0.0, 0.0);
        assert_eq!(op0, 0.0);
        assert_eq!(y0, motion::WHICH_KEY_IN_Y);
        assert!((s0 - motion::FLOAT_IN_SCALE).abs() < 1e-5);

        let (op1, y1, s1) = which_key_transform(true, 1.0, 1.0);
        assert_eq!(op1, 1.0);
        assert_eq!(y1, 0.0);
        assert!((s1 - 1.0).abs() < 1e-5);

        let (eop0, ey0, es0) = which_key_transform(false, 0.0, 0.0);
        assert_eq!(eop0, 0.0);
        assert_eq!(ey0, motion::WHICH_KEY_OUT_Y);
        assert!((es0 - motion::FLOAT_OUT_SCALE).abs() < 1e-5);
    }

    #[test]
    fn which_key_bounds_respect_viewport_caps() {
        let (min_w, max_w, list_h) = which_key_bounds(1600.0, 1000.0);
        assert_eq!(min_w, WHICH_KEY_MIN_WIDTH);
        assert_eq!(max_w, WHICH_KEY_MAX_WIDTH);
        assert!((list_h - (750.0 - WHICH_KEY_CHROME_HEIGHT)).abs() < 0.1);

        let (_, narrow_max_w, _) = which_key_bounds(400.0, 600.0);
        assert!((narrow_max_w - 360.0).abs() < 0.1);
    }
}
