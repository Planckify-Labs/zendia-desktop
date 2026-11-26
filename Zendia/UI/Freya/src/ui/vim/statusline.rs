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
/// Pointer-hover step-aside hysteresis margin (`8 px`, matching `ZendiaStatusline.sys.mjs:549`).
const STEP_ASIDE_HYSTERESIS: f32 = 8.0;
/// Stage width threshold below which the statusline auto-shrinks even under `show = "always"` (`VIM-SL-07`).
pub const SMALL_WINDOW_WIDTH: f32 = 700.0;
/// Upward rise in pixels while a key sequence is being typed.
const TYPING_RISE_PX: f32 = -4.0;

/// Window-local view state shared between the statusline, which-key popup, page scroll events, and side panels.
#[derive(Clone, Copy, PartialEq)]
pub struct VimStatuslineState {
    /// True while the active page is scrolling (ducks the statusline out of sight).
    pub scrolling: State<bool>,
    scroll_seq: State<u64>,
    /// Measured `(width, height)` of the active which-key popup at the statusline's corner (`(0, 0)` when closed).
    pub which_key_rect: State<(f32, f32)>,
    /// Width reserved on the statusline's corner edge by floating/sliding side panels (`ZendiaDockFrame`).
    pub side_panel_clearance: State<f32>,
    /// Optional floating overlay rect `(x, y, w, h)` in stage coordinates (e.g. picker or model card) to dodge in 2D.
    pub overlay_obstacle: State<Option<(f32, f32, f32, f32)>>,
    /// True while the window is in DOM fullscreen (`VIM-SL-07`).
    pub fullscreen: State<bool>,
    /// Monotonic counter bumped on external wake events (e.g. window activation, `VIM-SL-07`).
    pub wake_bump: State<u64>,
    /// Whether the view-level "all keymaps" (`<leader>?`) popup is open in this window.
    pub all_keymaps_open: State<bool>,
    /// Mode captured when `WhichKeyAllRequested` fired.
    pub all_keymaps_mode: State<InputMode>,
    /// Measured `(width, height)` of the page stage overlay in logical pixels.
    pub stage_size: State<(f32, f32)>,
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
        self.set_which_key_rect(width, 0.0);
    }

    /// Updates the width and height occupied by the which-key popup at the statusline's corner (`0.0, 0.0` when closed).
    pub fn set_which_key_rect(&self, width: f32, height: f32) {
        let mut wk_rect = self.which_key_rect;
        let w = if width.is_finite() && width > 0.0 { width } else { 0.0 };
        let h = if height.is_finite() && height > 0.0 {
            height
        } else {
            0.0
        };
        let (pw, ph) = *wk_rect.peek();
        if (pw - w).abs() > 0.5 || (ph - h).abs() > 0.5 {
            wk_rect.set((w, h));
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

    /// Declares an active floating overlay rect `(x, y, w, h)` in stage coordinates to dodge in 2D (`VIM-SL-05`).
    #[allow(dead_code)]
    pub fn set_overlay_obstacle(&self, rect: Option<(f32, f32, f32, f32)>) {
        let mut obstacle = self.overlay_obstacle;
        if *obstacle.peek() != rect {
            obstacle.set(rect);
        }
    }

    /// Updates fullscreen visibility suppression (`VIM-SL-07`).
    #[allow(dead_code)]
    pub fn set_fullscreen(&self, fullscreen: bool) {
        let mut fs = self.fullscreen;
        fs.set_if_modified(fullscreen);
    }

    /// Bumps the wake counter so the statusline rises to full size for `STATUSLINE_ACTIVE_MS` (`VIM-SL-07`).
    #[allow(dead_code)]
    pub fn wake(&self) {
        let mut bump = self.wake_bump;
        let next = *bump.peek() + 1;
        bump.set(next);
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

    /// Updates the measured `(width, height)` of the vim overlay stage.
    pub fn set_stage_size(&self, width: f32, height: f32) {
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return;
        }
        let mut size = self.stage_size;
        let (prev_w, prev_h) = *size.peek();
        if (prev_w - width).abs() > 0.5 || (prev_h - height).abs() > 0.5 {
            size.set((width, height));
        }
    }
}

/// Installs [`VimStatuslineState`] into Freya context for the current window or gallery.
pub fn use_provide_vim_statusline_state() -> VimStatuslineState {
    let scrolling = use_state(|| false);
    let scroll_seq = use_state(|| 0u64);
    let which_key_rect = use_state(|| (0.0f32, 0.0f32));
    let side_panel_clearance = use_state(|| 0.0f32);
    let overlay_obstacle = use_state(|| None::<(f32, f32, f32, f32)>);
    let fullscreen = use_state(|| false);
    let wake_bump = use_state(|| 0u64);
    let all_keymaps_open = use_state(|| false);
    let all_keymaps_mode = use_state(|| InputMode::Normal);
    let stage_size = use_state(|| (0.0f32, 0.0f32));
    use_provide_context(|| VimStatuslineState {
        scrolling,
        scroll_seq,
        which_key_rect,
        side_panel_clearance,
        overlay_obstacle,
        fullscreen,
        wake_bump,
        all_keymaps_open,
        all_keymaps_mode,
        stage_size,
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

/// Resolves the effective statusline & window-tint mode: Explorer with active multi-row selection renders as purple
/// `Visual` (`v`), matching `ZendiaStatusline.sys.mjs:743` (`VIM-SL-06`).
pub fn effective_statusline_mode(mode: InputMode, explorer_selected_empty: bool) -> InputMode {
    if mode == InputMode::Explorer && !explorer_selected_empty {
        InputMode::Visual
    } else {
        mode
    }
}

/// Resolves the statusline's presence from settings, activity, scroll ducking, pointer proximity, stage width, and
/// fullscreen state (`VIM-SL-07`).
#[allow(clippy::too_many_arguments)]
pub fn resolve_presence_with_window(
    vim_enabled: bool,
    statusline_enabled: bool,
    show: StatuslineShow,
    scrolling: bool,
    is_typing_or_announcing: bool,
    recently_active: bool,
    pointer_near: bool,
    stage_width: f32,
    fullscreen: bool,
) -> StatuslinePresence {
    if !vim_enabled || !statusline_enabled || fullscreen {
        return StatuslinePresence::Gone;
    }
    if scrolling {
        return StatuslinePresence::Tucked;
    }
    if is_typing_or_announcing || recently_active || pointer_near {
        return StatuslinePresence::Active;
    }
    let effective_show = if stage_width > 0.0 && stage_width < SMALL_WINDOW_WIDTH && show == StatuslineShow::Always {
        StatuslineShow::Shrink
    } else {
        show
    };
    match effective_show {
        StatuslineShow::Always => StatuslinePresence::Active,
        StatuslineShow::Shrink => StatuslinePresence::Resting,
        StatuslineShow::Never => StatuslinePresence::Gone,
    }
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
    resolve_presence_with_window(
        vim_enabled,
        statusline_enabled,
        show,
        scrolling,
        is_typing_or_announcing,
        recently_active,
        pointer_near,
        1280.0,
        false,
    )
}

/// Returns whether the pointer `(px, py)` is inside the home zone with `8px` hysteresis (`VIM-SL-03`).
pub fn should_step_aside(px: f32, py: f32, home_box: (f32, f32, f32, f32), currently_dodged: bool) -> bool {
    let (x0, y0, w, h) = home_box;
    if w <= 0.0 || h <= 0.0 {
        return false;
    }
    let margin = if currently_dodged { STEP_ASIDE_HYSTERESIS } else { 0.0 };
    px >= x0 - margin && px <= x0 + w + margin && py >= y0 - margin && py <= y0 + h + margin
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

/// 2D obstacle solver (`step = 8px`, `clearance = 12px`, `1.5x` horizontal preference with vertical fallback)
/// returning `(inward_x, upward_y)` offsets from the resting bottom corner (`VIM-SL-05`).
pub fn solve_2d_dodge(
    stage_w: f32,
    stage_h: f32,
    box_w: f32,
    box_h: f32,
    on_right: bool,
    obstacles: &[(f32, f32, f32, f32)],
) -> (f32, f32) {
    if obstacles.is_empty() || stage_w <= 0.0 || stage_h <= 0.0 || box_w <= 0.0 || box_h <= 0.0 {
        return (0.0, 0.0);
    }
    let overlaps = |ix: f32, uy: f32| -> bool {
        let bx = if on_right {
            stage_w - STATUSLINE_CORNER_GAP - ix - box_w
        } else {
            STATUSLINE_CORNER_GAP + ix
        };
        let by = stage_h - STATUSLINE_CORNER_GAP - uy - box_h;
        obstacles.iter().any(|&(ox, oy, ow, oh)| {
            ow > 0.5
                && oh > 0.5
                && bx < ox + ow + STATUSLINE_CLEARANCE
                && bx + box_w + STATUSLINE_CLEARANCE > ox
                && by < oy + oh + STATUSLINE_CLEARANCE
                && by + box_h + STATUSLINE_CLEARANCE > oy
        })
    };
    if !overlaps(0.0, 0.0) {
        return (0.0, 0.0);
    }
    let max_ix = (stage_w - 2.0 * STATUSLINE_CORNER_GAP - box_w).max(0.0);
    let max_uy = (stage_h - 2.0 * STATUSLINE_CORNER_GAP - box_h).max(0.0);
    let mut best: Option<(f32, f32, f32)> = None;
    let step = 8.0f32;
    let mut uy = 0.0f32;
    while uy <= max_uy + 0.1 {
        let mut ix = 0.0f32;
        while ix <= max_ix + 0.1 {
            if !overlaps(ix, uy) {
                // Prefer horizontal shift by 1.5x over vertical shift, matching `ZendiaStatusline.sys.mjs:631-736`.
                let cost = ix + uy * 1.5;
                if best.is_none_or(|(c, _, _)| cost < c) {
                    best = Some((cost, ix, uy));
                }
                break;
            }
            ix += step;
        }
        uy += step;
    }
    best.map(|(_, ix, uy)| (ix, uy)).unwrap_or((0.0, 0.0))
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

/// Formats typed keys plus an optional `+group` suffix from the open which-key group (`VIM-SL-08`).
pub fn format_typed_keys_with_group(count: Option<u32>, pending_keys: &[String], group_title: Option<&str>) -> String {
    let mut base = format_typed_keys(count, pending_keys);
    if !base.is_empty()
        && let Some(raw) = group_title.map(str::trim)
    {
        let group = raw.rsplit_once(' ').map(|(_, tail)| tail).unwrap_or(raw);
        if group.starts_with('+') && group.len() > 1 {
            base.push(' ');
            base.push_str(group);
        }
    }
    base
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
        let mut last_activity_key = use_state(|| {
            None::<(
                InputMode,
                Option<u32>,
                Vec<String>,
                bool,
                Option<zendia_browser_core::model::TabId>,
                u64,
            )>
        });
        let mut last_announcement_id = use_state(|| None::<u64>);
        let mut pointer_near = use_state(|| false);
        let mut pointer_dodged = use_state(|| false);
        let mut statusline_box = use_state(|| (0.0f32, 0.0f32, 0.0f32, 0.0f32));
        let mut home_box = use_state(|| (0.0f32, 0.0f32, 0.0f32, 0.0f32));

        let (
            vim_enabled,
            mode,
            pending_count,
            pending_keys,
            wk_group_title,
            statusline_settings,
            announcement,
            sidebar_side,
            active_tab,
        ) = {
            let model = core.model().read();
            let eff_mode = effective_statusline_mode(model.input_mode, model.focus.selection.is_empty());
            let group_title = model.which_key.as_ref().map(|wk| wk.title.clone());
            (
                model.vim_mode_enabled,
                eff_mode,
                model.pending_count,
                model.pending_keys.clone(),
                group_title,
                model.statusline.clone(),
                model.statusline_announcement.clone(),
                model.layout.sidebar_side,
                model.active_tab,
            )
        };

        let typed = format_typed_keys_with_group(pending_count, &pending_keys, wk_group_title.as_deref());
        let has_typing = !typed.is_empty();
        let has_announcement = announcement.is_some();
        let wake_bump = vim_state.map(|s| *s.wake_bump.read()).unwrap_or(0);

        // Wake to full size for STATUSLINE_ACTIVE_MS on mode change, pending key completion, tab change, or vim toggle.
        let activity_key = (
            mode,
            pending_count,
            pending_keys.clone(),
            vim_enabled,
            active_tab,
            wake_bump,
        );
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
        let (wk_width, wk_height) = vim_state.map(|s| *s.which_key_rect.read()).unwrap_or((0.0, 0.0));
        let panel_clearance = vim_state.map(|s| *s.side_panel_clearance.read()).unwrap_or(0.0);
        let overlay_obstacle = vim_state.and_then(|s| *s.overlay_obstacle.read());
        let (stage_w, stage_h) = vim_state.map(|s| *s.stage_size.read()).unwrap_or((0.0, 0.0));
        let fullscreen = vim_state.is_some_and(|s| *s.fullscreen.read());

        let presence = if self.force_shrunk && vim_enabled && statusline_settings.enabled {
            StatuslinePresence::Resting
        } else if self.static_preview && vim_enabled && statusline_settings.enabled && !scrolling {
            match statusline_settings.show {
                StatuslineShow::Never if !has_typing && !has_announcement => StatuslinePresence::Gone,
                _ => StatuslinePresence::Active,
            }
        } else {
            resolve_presence_with_window(
                vim_enabled,
                statusline_settings.enabled,
                statusline_settings.show,
                scrolling,
                has_typing || has_announcement,
                *recently_active.read(),
                *pointer_near.read(),
                stage_w,
                fullscreen,
            )
        };

        // Per-part staggered jelly pop-in / pop-out (`VIM-SL-01`): keep mounted while popping out to scale 0.
        let popped_in = matches!(presence, StatuslinePresence::Active | StatuslinePresence::Resting);
        let mut last_pop_in = use_state(|| popped_in);
        let mut pop_entering = use_state(|| popped_in);
        if *last_pop_in.peek() != popped_in {
            last_pop_in.set(popped_in);
            pop_entering.set(popped_in);
        }
        let pop_anim = use_animation_with_dependencies(&popped_in, move |conf, entering| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            if *entering {
                AnimNum::new(0.0, 1.0)
                    .duration(motion::MASCOT_POP_IN)
                    .ease(Ease::Out)
                    .function(Function::Linear)
            } else {
                AnimNum::new(1.0, 0.0)
                    .duration(motion::MASCOT_POP_OUT)
                    .ease(Ease::In)
                    .function(Function::Linear)
            }
        });
        let pop_t = if self.force_ducked && self.static_preview {
            // Mid-pop-out snapshot for gallery preview so the staggered jelly collapse is clearly visible.
            0.35
        } else if self.static_preview {
            if popped_in { 1.0 } else { 0.0 }
        } else {
            pop_anim.get().value().clamp(0.0, 1.0)
        };

        // Smooth staggered shrink / grow spring between full mascot (1.0) and corner badge (0.4) (`VIM-SL-02`).
        let is_resting = presence == StatuslinePresence::Resting;
        let mut last_resting = use_state(|| is_resting);
        let mut shrink_growing = use_state(|| false);
        if *last_resting.peek() != is_resting {
            shrink_growing.set(*last_resting.peek() && !is_resting);
            last_resting.set(is_resting);
        }
        let shrink_anim = use_animation_with_dependencies(&is_resting, move |conf, resting| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *resting { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::MASCOT_SHRINK)
                .ease(Ease::Out)
                .function(motion::spring_fn())
        });
        let shrink_t = if self.static_preview {
            if is_resting { 1.0 } else { 0.0 }
        } else {
            shrink_anim.get().value().clamp(0.0, 1.05)
        };
        // Hold whole scale for the first 160ms (~0.31 of 520ms) while individual cells stagger (`VIM-SL-02`).
        let held_scale_t = if is_resting {
            ((shrink_t - 0.31) / 0.69).clamp(0.0, 1.0)
        } else {
            (shrink_t / 0.69).clamp(0.0, 1.0)
        };
        let animated_scale = (1.0 + (MASCOT_REST_SCALE - 1.0) * held_scale_t).clamp(0.25, 1.15);

        let mascot_size = (MASCOT_FULL_SIZE * animated_scale).round().max(32.0);
        let on_right = sidebar_side == SidebarSide::Left;

        // 2D obstacle dodging + pointer-hover step-aside to opposite corner (`VIM-SL-03`, `VIM-SL-05`).
        let is_stepped_aside = !self.static_preview && *pointer_dodged.read() && popped_in;
        let (target_dodge_x, target_dodge_y) = if is_stepped_aside && stage_w > mascot_size + 48.0 {
            let opposite_x = (stage_w - 2.0 * STATUSLINE_CORNER_GAP - mascot_size).max(0.0);
            (opposite_x, 0.0)
        } else if stage_w > 0.0 && stage_h > 0.0 {
            let mut obstacles = Vec::new();
            let edge_w = horizontal_dodge_offset(wk_width, panel_clearance, self.extra_panel_clearance);
            if edge_w > 1.0 {
                let edge_h = if wk_width > 1.0 && wk_height > 1.0 {
                    wk_height + STATUSLINE_CORNER_GAP
                } else {
                    stage_h
                };
                let ox = if on_right { (stage_w - edge_w).max(0.0) } else { 0.0 };
                let oy = (stage_h - edge_h).max(0.0);
                obstacles.push((ox, oy, (edge_w - STATUSLINE_CLEARANCE).max(1.0), edge_h));
            }
            if let Some(obs) = overlay_obstacle {
                obstacles.push(obs);
            }
            let (cur_bw, cur_bh) = {
                let (_, _, w, h) = *statusline_box.peek();
                if w > 1.0 && h > 1.0 {
                    (w, h)
                } else {
                    (mascot_size, mascot_size)
                }
            };
            solve_2d_dodge(stage_w, stage_h, cur_bw, cur_bh, on_right, &obstacles)
        } else {
            (
                horizontal_dodge_offset(wk_width, panel_clearance, self.extra_panel_clearance),
                0.0,
            )
        };

        let mut prev_dodge = use_state(|| (target_dodge_x, target_dodge_y));
        let mut from_dodge = use_state(|| (target_dodge_x, target_dodge_y));
        let mut dodge_seq = use_state(|| 0u64);
        let (pdx, pdy) = *prev_dodge.peek();
        if (pdx - target_dodge_x).abs() > 0.5 || (pdy - target_dodge_y).abs() > 0.5 {
            from_dodge.set((pdx, pdy));
            prev_dodge.set((target_dodge_x, target_dodge_y));
            let next = *dodge_seq.peek() + 1;
            dodge_seq.set(next);
        }
        let d_seq = *dodge_seq.read();
        let (dfx, dfy) = *from_dodge.read();
        let dodge_anim = use_animation_with_dependencies(
            &(
                d_seq,
                dfx.to_bits(),
                dfy.to_bits(),
                target_dodge_x.to_bits(),
                target_dodge_y.to_bits(),
            ),
            move |conf, _| {
                conf.on_creation(OnCreation::Finish);
                conf.on_change(OnChange::Rerun);
                AnimNum::new(0.0, 1.0)
                    .duration(motion::STATUSLINE_MOVE)
                    .ease(Ease::Out)
                    .function(motion::spring_fn())
            },
        );
        let (dodge_x, dodge_y) = if self.static_preview {
            (target_dodge_x, target_dodge_y)
        } else {
            let dt = dodge_anim.get().value();
            (
                (dfx + (target_dodge_x - dfx) * dt).max(0.0),
                (dfy + (target_dodge_y - dfy) * dt).max(0.0),
            )
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

        if !popped_in && pop_t <= 0.01 {
            return rect().width(Size::px(0.0)).height(Size::px(0.0));
        }

        let position = if on_right {
            Position::new_absolute()
                .right(STATUSLINE_CORNER_GAP + dodge_x)
                .bottom(STATUSLINE_CORNER_GAP + dodge_y)
        } else {
            Position::new_absolute()
                .left(STATUSLINE_CORNER_GAP + dodge_x)
                .bottom(STATUSLINE_CORNER_GAP + dodge_y)
        };

        let mode_color = mascot_mode_color(mode).to_color();
        let pill_bg = theme.color(ThemeRole::StatuslineBackground);
        let pill_fg = theme.color(ThemeRole::StatuslineText);
        let pill_border = theme.palette.ink.to_color();

        let keys_pill = has_typing.then(|| {
            rect()
                .padding(Gaps::new(1.0, 8.0, 1.0, 8.0))
                .margin(Gaps::new(0.0, 0.0, 2.0, 0.0))
                .corner_radius(CornerRadius::new_all(8.0))
                .background(pill_bg)
                .border(
                    Border::new()
                        .width(2.0)
                        .alignment(BorderAlignment::Inner)
                        .fill(pill_border),
                )
                .opacity(pop_t)
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
                .opacity(pop_t)
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

        // Visually-hidden accessible live-region announcer for mode switches (`VIM-SL-09`).
        let sr_announcer = rect()
            .width(Size::px(1.0))
            .height(Size::px(1.0))
            .opacity(0.0)
            .overflow(Overflow::Clip)
            .interactive(Interactive::No)
            .a11y_role(AccessibilityRole::Status)
            .a11y_alt(format!("{} mode", mode_name(mode)));

        let mascot_el = rect().offset_y(typing_rise).child(
            VimMascot::new(mode)
                .size(mascot_size)
                .pop(pop_t, *pop_entering.read())
                .shrink(shrink_t.clamp(0.0, 1.0), *shrink_growing.read())
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
            let current_box = *statusline_box.peek();
            let hb = *home_box.peek();
            let active_zone = if hb.2 > 0.0 && hb.3 > 0.0 { hb } else { current_box };
            let (x0, y0, w, h) = active_zone;
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
                // Keep full mascot awake for 2000ms on pointer wake AND after pointer leaves (`VIM-SL-04`).
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
            let dodged = should_step_aside(px, py, active_zone, *pointer_dodged.peek());
            if *pointer_dodged.peek() != dodged {
                pointer_dodged.set(dodged);
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
            .on_sized(move |e: Event<SizedEventData>| {
                let next = (e.area.min_x(), e.area.min_y(), e.area.width(), e.area.height());
                if *statusline_box.peek() != next {
                    statusline_box.set(next);
                }
                if !*pointer_dodged.peek() && *home_box.peek() != next {
                    home_box.set(next);
                }
            })
            .on_global_pointer_move(on_global_pointer_move)
            .child(sr_announcer);

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
        assert_eq!(
            format_typed_keys_with_group(None, &["<Space>".to_string(), "a".to_string()], Some("+ai")),
            "<Space>a +ai"
        );
        assert_eq!(
            format_typed_keys_with_group(None, &["<Space>".to_string(), "a".to_string()], Some("SPC » a +ai")),
            "<Space>a +ai"
        );
        assert_eq!(
            format_typed_keys_with_group(None, &["<Space>".to_string()], Some("SPC")),
            "<Space>"
        );
        assert_eq!(effective_statusline_mode(InputMode::Explorer, false), InputMode::Visual);
        assert_eq!(
            effective_statusline_mode(InputMode::Explorer, true),
            InputMode::Explorer
        );
        assert_eq!(
            resolve_presence_with_window(
                true,
                true,
                StatuslineShow::Always,
                false,
                false,
                false,
                false,
                640.0,
                false
            ),
            StatuslinePresence::Resting
        );
        assert_eq!(
            resolve_presence_with_window(
                true,
                true,
                StatuslineShow::Always,
                false,
                false,
                true,
                false,
                1280.0,
                true
            ),
            StatuslinePresence::Gone
        );
        // Step aside with 8px hysteresis.
        assert!(should_step_aside(1050.0, 750.0, (1000.0, 700.0, 132.0, 132.0), false));
        assert!(should_step_aside(995.0, 750.0, (1000.0, 700.0, 132.0, 132.0), true));
        assert!(!should_step_aside(995.0, 750.0, (1000.0, 700.0, 132.0, 132.0), false));
        // 2D obstacle solver: prefers horizontal dodge, falls back vertically when horizontal is blocked.
        let (dx, dy) = solve_2d_dodge(1000.0, 700.0, 132.0, 132.0, true, &[(720.0, 420.0, 268.0, 268.0)]);
        assert!(dx > 200.0 && dy == 0.0, "expected horizontal dodge, got ({dx}, {dy})");
        let (dx2, dy2) = solve_2d_dodge(400.0, 700.0, 132.0, 132.0, true, &[(0.0, 500.0, 400.0, 200.0)]);
        assert!(dy2 > 150.0, "expected vertical fallback, got ({dx2}, {dy2})");
    }
}
