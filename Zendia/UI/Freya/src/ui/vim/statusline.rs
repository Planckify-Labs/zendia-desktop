/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Vim mode indicator / statusline and per-mode window frame tint.
//!
//! Renders Zendia's animated mascot in the bottom corner opposite the sidebar alongside typed keys/count and short
//! transient announcements. Rests shrunk to a corner badge after the quiet timeout (`zendia.statusline.show = "shrink"`),
//! ducks while the page scrolls, and slides clear of which-key and Zendia side panels.

use crate::ui::chrome::layout::BackdropEndpoints;
use crate::ui::core_view::use_core;
use crate::ui::theme::{CHROME_OVERLAY_LEVEL, ResolvedScheme, ResolvedTheme, Rgba, ThemeRole, motion, use_theme};
use crate::ui::vim::mascot::{MASCOT_FULL_SIZE, MASCOT_REST_SCALE, VimMascot, mascot_mode_color};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;
use zendia_browser_core::commands::Command;
use zendia_browser_core::input_state::{StatuslineShow, mode_name};
use zendia_browser_core::model::{InputMode, SidebarSide};

/// Margin from the page card's bottom and side edges in logical pixels.
pub const STATUSLINE_CORNER_GAP: f32 = 12.0;
/// Clearance kept between the statusline and any floating panel (which-key, side panels).
pub const STATUSLINE_CLEARANCE: f32 = 12.0;
/// Quiet timeout before shrinking to a badge (`zendia.statusline.show = "shrink"`) or hiding (`"never"`).
pub const STATUSLINE_ACTIVE_MS: Duration = Duration::from_millis(2000);
/// Time after page wheel scrolling stops before the statusline returns from ducking.
pub const STATUSLINE_SCROLL_SETTLE: Duration = Duration::from_millis(500);
/// Distance from the resting corner within which the pointer wakes the shrunk badge.
const POINTER_NEAR_PX: f32 = 64.0;
/// Upward rise in pixels while a key sequence is being typed.
const TYPING_RISE_PX: f32 = -4.0;
/// Downward travel in pixels when ducking while the page scrolls.
const DUCK_DROP_PX: f32 = 28.0;

/// Window-local view state shared between the statusline, which-key popup, page scroll events, and future Phase 6
/// side panels.
#[derive(Clone, Copy, PartialEq)]
pub struct VimStatuslineState {
    /// True while the active page is scrolling (ducks the statusline out of sight).
    pub scrolling: State<bool>,
    scroll_seq: State<u64>,
    /// Measured width of the active which-key popup, so the statusline slides cleanly beside it.
    pub which_key_width: State<f32>,
    /// Clear seam for Phase 6 side panels (`ZendiaDockFrame`): width reserved on the statusline's corner edge.
    pub side_panel_clearance: State<f32>,
    /// Whether the view-level "all keymaps" (`<leader>?`) popup is open in this window.
    pub all_keymaps_open: State<bool>,
    /// Mode captured when `WhichKeyAllRequested` fired.
    pub all_keymaps_mode: State<InputMode>,
}

impl VimStatuslineState {
    /// Marks the active page as scrolling so the statusline ducks until scrolling settles.
    pub fn notify_page_scroll(&self) {
        let mut scrolling = self.scrolling;
        let mut scroll_seq = self.scroll_seq;
        let seq = *scroll_seq.peek() + 1;
        scroll_seq.set(seq);
        scrolling.set_if_modified(true);
        spawn(async move {
            timer(STATUSLINE_SCROLL_SETTLE).await;
            if *scroll_seq.peek() == seq {
                scrolling.set_if_modified(false);
            }
        });
    }

    /// Updates the width occupied by the which-key popup at the statusline's corner (`0.0` when closed).
    pub fn set_which_key_width(&self, width: f32) {
        let mut wk_width = self.which_key_width;
        let clamped = if width.is_finite() && width > 0.0 { width } else { 0.0 };
        if (*wk_width.peek() - clamped).abs() > 0.5 {
            wk_width.set(clamped);
        }
    }

    /// Seam for Phase 6 side panels to declare horizontal clearance on the statusline's corner edge.
    #[allow(dead_code)]
    pub fn set_side_panel_clearance(&self, width: f32) {
        let mut clearance = self.side_panel_clearance;
        let clamped = if width.is_finite() && width > 0.0 { width } else { 0.0 };
        if (*clearance.peek() - clamped).abs() > 0.5 {
            clearance.set(clamped);
        }
    }

    /// Opens the view-level all-keymaps popup (`<leader>?`) for `mode`.
    pub fn open_all_keymaps(&self, mode: InputMode) {
        let mut open = self.all_keymaps_open;
        let mut saved_mode = self.all_keymaps_mode;
        saved_mode.set_if_modified(mode);
        open.set_if_modified(true);
    }

    /// Closes the view-level all-keymaps popup.
    pub fn close_all_keymaps(&self) {
        let mut open = self.all_keymaps_open;
        open.set_if_modified(false);
    }
}

/// Installs [`VimStatuslineState`] into Freya context for the current window or gallery.
pub fn use_provide_vim_statusline_state() -> VimStatuslineState {
    let scrolling = use_state(|| false);
    let scroll_seq = use_state(|| 0u64);
    let which_key_width = use_state(|| 0.0f32);
    let side_panel_clearance = use_state(|| 0.0f32);
    let all_keymaps_open = use_state(|| false);
    let all_keymaps_mode = use_state(|| InputMode::Normal);
    use_provide_context(|| VimStatuslineState {
        scrolling,
        scroll_seq,
        which_key_width,
        side_panel_clearance,
        all_keymaps_open,
        all_keymaps_mode,
    })
}

/// Reads [`VimStatuslineState`] from Freya context when installed.
pub fn try_use_vim_statusline_state() -> Option<VimStatuslineState> {
    try_consume_context::<VimStatuslineState>()
}

/// Computes the per-mode window frame backdrop gradient when vim mode is enabled.
///
/// Mirrors Zen's single-dot space gradient tinted with the mode's bright Tokyonight hue (pink NORMAL, green INSERT,
/// purple VISUAL, blue EXPLORER, yellow TOOLBAR, teal LIBRARY, orange WINDOW) so the sidebar and window frame tint
/// clearly identify the active vim mode in both dark and light themes.
pub fn vim_mode_backdrop_endpoints(
    mode: InputMode,
    scheme: ResolvedScheme,
    _theme: &ResolvedTheme,
) -> BackdropEndpoints {
    let mode_rgb = mascot_mode_color(mode);
    match scheme {
        ResolvedScheme::Dark => {
            let base_start = Rgba::rgb(22, 23, 31);
            let base_end = Rgba::rgb(17, 18, 25);
            BackdropEndpoints {
                start: base_start.mix(mode_rgb, 0.27),
                end: base_end.mix(mode_rgb, 0.21),
                angle_deg: 165.0,
            }
        }
        ResolvedScheme::Light => {
            let base_start = Rgba::rgb(248, 248, 252);
            let base_end = Rgba::rgb(242, 243, 248);
            let lifted = mode_rgb.mix(Rgba::rgb(255, 255, 255), 0.56);
            BackdropEndpoints {
                start: base_start.mix(lifted, 0.68),
                end: base_end.mix(lifted, 0.56),
                angle_deg: 165.0,
            }
        }
    }
}

/// Resting/active visual presence of the statusline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatuslinePresence {
    Active,
    Resting,
    Tucked,
    Gone,
}

/// Resolves the statusline's presence from settings, activity, scroll ducking, and pointer proximity.
pub fn resolve_presence(
    vim_enabled: bool,
    statusline_enabled: bool,
    show: StatuslineShow,
    scrolling: bool,
    is_typing_or_announcing: bool,
    recently_active: bool,
    pointer_near: bool,
) -> StatuslinePresence {
    if !vim_enabled || !statusline_enabled {
        return StatuslinePresence::Gone;
    }
    if scrolling {
        return StatuslinePresence::Tucked;
    }
    if is_typing_or_announcing || recently_active || pointer_near {
        return StatuslinePresence::Active;
    }
    match show {
        StatuslineShow::Always => StatuslinePresence::Active,
        StatuslineShow::Shrink => StatuslinePresence::Resting,
        StatuslineShow::Never => StatuslinePresence::Gone,
    }
}

/// Computes the horizontal dodge offset (always `>= 0.0`, applied inward from the corner) so the statusline stays
/// clear of which-key and any docked side panel.
pub fn horizontal_dodge_offset(which_key_width: f32, side_panel_clearance: f32, extra_dodge: f32) -> f32 {
    let wk = if which_key_width > 1.0 {
        which_key_width + STATUSLINE_CLEARANCE
    } else {
        0.0
    };
    let panel = if side_panel_clearance > 1.0 {
        side_panel_clearance + STATUSLINE_CLEARANCE
    } else {
        0.0
    };
    wk.max(panel).max(extra_dodge.max(0.0))
}

/// Formats the typed count + pending keys pill string (e.g. `3g`, `<Space>`, `<Space>a`).
pub fn format_typed_keys(count: Option<u32>, pending_keys: &[String]) -> String {
    let mut out = String::new();
    if let Some(n) = count {
        out.push_str(&n.to_string());
    }
    for token in pending_keys {
        out.push_str(token);
    }
    out
}

/// Vim mode indicator and mascot floated above the page at [`CHROME_OVERLAY_LEVEL`].
#[derive(Clone, PartialEq)]
pub struct VimStatusline {
    /// Forces shrunk badge state (deterministic gallery preview).
    pub force_shrunk: bool,
    /// Forces scroll-ducked state (deterministic gallery preview).
    pub force_ducked: bool,
    /// Additional horizontal panel clearance (e.g. simulating a Phase 6 side panel in the gallery).
    pub extra_panel_clearance: f32,
    /// Disables auto-timers and blinking for deterministic gallery screenshots.
    pub static_preview: bool,
    pub key: DiffKey,
}

impl Default for VimStatusline {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyExt for VimStatusline {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl VimStatusline {
    pub fn new() -> Self {
        Self {
            force_shrunk: false,
            force_ducked: false,
            extra_panel_clearance: 0.0,
            static_preview: false,
            key: DiffKey::None,
        }
    }

    pub fn force_shrunk(mut self, shrunk: bool) -> Self {
        self.force_shrunk = shrunk;
        self
    }

    pub fn force_ducked(mut self, ducked: bool) -> Self {
        self.force_ducked = ducked;
        self
    }

    pub fn static_preview(mut self, static_preview: bool) -> Self {
        self.static_preview = static_preview;
        self
    }
}

impl Component for VimStatusline {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let vim_state = try_use_vim_statusline_state();

        // Hooks always run unconditionally at the top of render.
        let mut recently_active = use_state(|| true);
        let mut wake_seq = use_state(|| 0u64);
        let mut last_activity_key = use_state(|| None::<(InputMode, Option<u32>, Vec<String>, bool)>);
        let mut last_announcement_id = use_state(|| None::<u64>);
        let mut pointer_near = use_state(|| false);
        let mut statusline_box = use_state(|| (0.0f32, 0.0f32, 0.0f32, 0.0f32));

        let (vim_enabled, mode, pending_count, pending_keys, statusline_settings, announcement, sidebar_side) = {
            let model = core.model().read();
            (
                model.vim_mode_enabled,
                model.input_mode,
                model.pending_count,
                model.pending_keys.clone(),
                model.statusline.clone(),
                model.statusline_announcement.clone(),
                model.layout.sidebar_side,
            )
        };

        let typed = format_typed_keys(pending_count, &pending_keys);
        let has_typing = !typed.is_empty();
        let has_announcement = announcement.is_some();

        // Wake to full size for STATUSLINE_ACTIVE_MS on mode change, pending key completion, or vim toggle.
        let activity_key = (mode, pending_count, pending_keys.clone(), vim_enabled);
        if *last_activity_key.peek() != Some(activity_key.clone()) {
            last_activity_key.set(Some(activity_key));
            if !self.static_preview && !self.force_shrunk {
                let seq = *wake_seq.peek() + 1;
                wake_seq.set(seq);
                recently_active.set_if_modified(true);
                spawn(async move {
                    timer(STATUSLINE_ACTIVE_MS).await;
                    if *wake_seq.peek() == seq {
                        recently_active.set_if_modified(false);
                    }
                });
            }
        }

        // Auto-clear transient announcements after their timeout (unless in static gallery preview).
        let current_announcement_id = announcement.as_ref().map(|a| a.id);
        if *last_announcement_id.peek() != current_announcement_id {
            last_announcement_id.set(current_announcement_id);
            if !self.static_preview
                && let Some(ref ann) = announcement
            {
                let clear_id = ann.id;
                let timeout = Duration::from_millis(ann.timeout_ms.max(400));
                let core = core.clone();
                spawn(async move {
                    timer(timeout).await;
                    if core.model().peek().statusline_announcement.as_ref().map(|a| a.id) == Some(clear_id) {
                        core.invoke_or_log(Command::StatuslineClearAnnouncement);
                    }
                });
            }
        }

        let scrolling = self.force_ducked || vim_state.is_some_and(|s| *s.scrolling.read());
        let wk_width = vim_state.map(|s| *s.which_key_width.read()).unwrap_or(0.0);
        let panel_clearance = vim_state.map(|s| *s.side_panel_clearance.read()).unwrap_or(0.0);

        let presence = if self.force_shrunk && vim_enabled && statusline_settings.enabled {
            StatuslinePresence::Resting
        } else if self.static_preview && vim_enabled && statusline_settings.enabled && !scrolling {
            match statusline_settings.show {
                StatuslineShow::Never if !has_typing && !has_announcement => StatuslinePresence::Gone,
                _ => StatuslinePresence::Active,
            }
        } else {
            resolve_presence(
                vim_enabled,
                statusline_settings.enabled,
                statusline_settings.show,
                scrolling,
                has_typing || has_announcement,
                *recently_active.read(),
                *pointer_near.read(),
            )
        };

        // Smooth scale between resting badge (0.4) and full size (1.0).
        let target_scale = match presence {
            StatuslinePresence::Resting => MASCOT_REST_SCALE,
            _ => 1.0,
        };
        let mut prev_scale = use_state(|| target_scale);
        let mut from_scale = use_state(|| target_scale);
        let mut scale_seq = use_state(|| 0u64);
        if (*prev_scale.peek() - target_scale).abs() > 0.001 {
            let old = *prev_scale.peek();
            from_scale.set(old);
            prev_scale.set(target_scale);
            let next = *scale_seq.peek() + 1;
            scale_seq.set(next);
        }
        let s_seq = *scale_seq.read();
        let s_from = *from_scale.read();
        let scale_anim =
            use_animation_with_dependencies(&(s_seq, s_from.to_bits(), target_scale.to_bits()), move |conf, _| {
                conf.on_creation(OnCreation::Finish);
                conf.on_change(OnChange::Rerun);
                AnimNum::new(s_from, target_scale)
                    .duration(motion::STATUSLINE_MOVE)
                    .ease(Ease::Out)
                    .function(motion::spring_fn())
            });
        let animated_scale = if self.static_preview {
            target_scale
        } else {
            scale_anim.get().value().clamp(0.25, 1.15)
        };

        // Smooth ducking while page scrolls (160 ms).
        let is_ducked = presence == StatuslinePresence::Tucked;
        let duck_anim = use_animation_with_dependencies(&is_ducked, move |conf, ducked| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *ducked { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::STATUSLINE_DUCK)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let duck_t = if self.static_preview {
            if is_ducked { 1.0 } else { 0.0 }
        } else {
            duck_anim.get().value().clamp(0.0, 1.0)
        };

        // Smooth horizontal dodge around which-key and docked side panels (340 ms spring).
        let target_dodge = horizontal_dodge_offset(wk_width, panel_clearance, self.extra_panel_clearance);
        let mut prev_dodge = use_state(|| target_dodge);
        let mut from_dodge = use_state(|| target_dodge);
        let mut dodge_seq = use_state(|| 0u64);
        if (*prev_dodge.peek() - target_dodge).abs() > 0.5 {
            let old = *prev_dodge.peek();
            from_dodge.set(old);
            prev_dodge.set(target_dodge);
            let next = *dodge_seq.peek() + 1;
            dodge_seq.set(next);
        }
        let d_seq = *dodge_seq.read();
        let d_from = *from_dodge.read();
        let dodge_anim =
            use_animation_with_dependencies(&(d_seq, d_from.to_bits(), target_dodge.to_bits()), move |conf, _| {
                conf.on_creation(OnCreation::Finish);
                conf.on_change(OnChange::Rerun);
                AnimNum::new(d_from, target_dodge)
                    .duration(motion::STATUSLINE_MOVE)
                    .ease(Ease::Out)
                    .function(motion::spring_fn())
            });
        let dodge_x = if self.static_preview {
            target_dodge
        } else {
            dodge_anim.get().value().max(0.0)
        };

        // Slight upward rise when typing a key chord (160 ms).
        let rise_anim = use_animation_with_dependencies(&has_typing, move |conf, typing| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *typing {
                (0.0, TYPING_RISE_PX)
            } else {
                (TYPING_RISE_PX, 0.0)
            };
            AnimNum::new(from, to)
                .duration(motion::STATUSLINE_DUCK)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });
        let typing_rise = if self.static_preview {
            if has_typing { TYPING_RISE_PX } else { 0.0 }
        } else {
            rise_anim.get().value()
        };

        if presence == StatuslinePresence::Gone {
            return rect().width(Size::px(0.0)).height(Size::px(0.0));
        }

        let mascot_size = (MASCOT_FULL_SIZE * animated_scale).round().max(32.0);
        let on_right = sidebar_side == SidebarSide::Left;
        let position = if on_right {
            Position::new_absolute()
                .right(STATUSLINE_CORNER_GAP + dodge_x)
                .bottom(STATUSLINE_CORNER_GAP)
        } else {
            Position::new_absolute()
                .left(STATUSLINE_CORNER_GAP + dodge_x)
                .bottom(STATUSLINE_CORNER_GAP)
        };

        let opacity = if self.force_ducked {
            0.22
        } else {
            (1.0 - duck_t).clamp(0.0, 1.0)
        };
        let y_offset = if self.force_ducked {
            DUCK_DROP_PX * 0.65
        } else {
            DUCK_DROP_PX * duck_t
        };

        let mode_color = mascot_mode_color(mode).to_color();
        let pill_bg = theme.color(ThemeRole::StatuslineBackground);
        let pill_fg = theme.color(ThemeRole::StatuslineText);
        let pill_border = theme.palette.ink.to_color();

        let keys_pill = has_typing.then(|| {
            rect()
                .padding(Gaps::new(2.0, 8.0, 2.0, 8.0))
                .margin(Gaps::new(0.0, 0.0, 2.0, 0.0))
                .corner_radius(CornerRadius::new_all(8.0))
                .background(pill_bg)
                .border(
                    Border::new()
                        .width(2.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(pill_border),
                )
                .child(
                    label()
                        .text(typed)
                        .font_size(12.0)
                        .font_weight(FontWeight::BOLD)
                        .font_family("monospace")
                        .max_lines(1)
                        .color(pill_fg),
                )
        });

        let announcement_pill = announcement.map(|ann| {
            rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::center())
                .spacing(6.0)
                .padding(Gaps::new(4.0, 10.0, 4.0, 9.0))
                .margin(Gaps::new(0.0, 0.0, 2.0, 0.0))
                .corner_radius(CornerRadius::new_all(10.0))
                .background(pill_bg)
                .border(
                    Border::new()
                        .width(1.5)
                        .alignment(BorderAlignment::Inner)
                        .fill(mode_color),
                )
                .shadow(Shadow::new().y(2.0).blur(10.0).color(Color::from_argb(90, 0, 0, 0)))
                .child(
                    rect()
                        .width(Size::px(7.0))
                        .height(Size::px(7.0))
                        .corner_radius(CornerRadius::new_all(7.0))
                        .background(mode_color),
                )
                .child(
                    label()
                        .text(ann.message)
                        .font_size(12.0)
                        .font_weight(FontWeight::MEDIUM)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis)
                        .color(pill_fg),
                )
        });

        let mascot_el = rect().offset_y(typing_rise).child(
            VimMascot::new(mode)
                .size(mascot_size)
                .disable_blink(self.static_preview),
        );

        let pills_group = (keys_pill.is_some() || announcement_pill.is_some()).then(|| {
            rect()
                .direction(Direction::Horizontal)
                .cross_align(Alignment::End)
                .spacing(6.0)
                .maybe_child(announcement_pill)
                .maybe_child(keys_pill)
        });

        let static_preview = self.static_preview;
        let on_global_pointer_move = move |e: Event<PointerEventData>| {
            if static_preview {
                return;
            }
            let (x0, y0, w, h) = *statusline_box.peek();
            if w <= 0.0 || h <= 0.0 {
                return;
            }
            let px = e.global_location().x as f32;
            let py = e.global_location().y as f32;
            let near = px >= x0 - POINTER_NEAR_PX
                && px <= x0 + w + POINTER_NEAR_PX
                && py >= y0 - POINTER_NEAR_PX
                && py <= y0 + h + POINTER_NEAR_PX;
            if *pointer_near.peek() != near {
                pointer_near.set(near);
            }
        };

        let mut container = rect()
            .layer(Layer::OverlayLevel(CHROME_OVERLAY_LEVEL))
            .position(position)
            .interactive(Interactive::No)
            .a11y_role(AccessibilityRole::Status)
            .a11y_alt(mode_name(mode))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::End)
            .spacing(6.0)
            .opacity(opacity)
            .offset_y(y_offset)
            .on_sized(move |e: Event<SizedEventData>| {
                let next = (e.area.min_x(), e.area.min_y(), e.area.width(), e.area.height());
                if *statusline_box.peek() != next {
                    statusline_box.set(next);
                }
            })
            .on_global_pointer_move(on_global_pointer_move);

        if on_right {
            container = container.maybe_child(pills_group).child(mascot_el);
        } else {
            container = container.child(mascot_el).maybe_child(pills_group);
        }
        container
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_statusline_presence_across_settings_and_activity() {
        assert_eq!(
            resolve_presence(false, true, StatuslineShow::Always, false, false, true, false),
            StatuslinePresence::Gone
        );
        assert_eq!(
            resolve_presence(true, true, StatuslineShow::Shrink, true, true, true, true),
            StatuslinePresence::Tucked
        );
        assert_eq!(
            resolve_presence(true, true, StatuslineShow::Shrink, false, false, false, false),
            StatuslinePresence::Resting
        );
        assert_eq!(
            resolve_presence(true, true, StatuslineShow::Shrink, false, false, false, true),
            StatuslinePresence::Active
        );
        assert_eq!(
            resolve_presence(true, true, StatuslineShow::Always, false, false, false, false),
            StatuslinePresence::Active
        );
        assert_eq!(
            resolve_presence(true, true, StatuslineShow::Never, false, false, false, false),
            StatuslinePresence::Gone
        );
        assert_eq!(
            resolve_presence(true, true, StatuslineShow::Never, false, true, false, false),
            StatuslinePresence::Active
        );
    }

    #[test]
    fn horizontal_dodge_clears_which_key_and_panels() {
        assert_eq!(horizontal_dodge_offset(0.0, 0.0, 0.0), 0.0);
        assert_eq!(horizontal_dodge_offset(320.0, 0.0, 0.0), 332.0);
        assert_eq!(horizontal_dodge_offset(200.0, 360.0, 0.0), 372.0);
    }

    #[test]
    fn formats_count_and_pending_keys() {
        assert_eq!(format_typed_keys(Some(3), &["g".to_string()]), "3g");
        assert_eq!(
            format_typed_keys(None, &["<Space>".to_string(), "a".to_string()]),
            "<Space>a"
        );
        assert_eq!(format_typed_keys(None, &[]), "");
    }
}
