/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Zendia's motion timings, distances, and easing functions.
//!
//! Values match the fork's floating surfaces, dock frame, tab transitions, and agent motion. All easing functions are
//! pure and compatible with Freya's [`freya::animation::Function::Fn`].

use freya::animation::{Ease, Function};
use std::time::Duration;

/// Hover color/background feedback duration (`120 ms`).
pub const HOVER: Duration = Duration::from_millis(120);
/// Button / row press scale feedback duration (`100 ms`).
pub const PRESS_DURATION: Duration = Duration::from_millis(100);
/// Tooltip hover show delay (`500 ms`).
pub const TOOLTIP_DELAY: Duration = Duration::from_millis(500);
/// Tooltip fade-in/out duration (`120 ms`).
pub const TOOLTIP_FADE: Duration = Duration::from_millis(120);
/// Floating panel / picker / popover entrance duration (`300 ms`).
pub const FLOAT_IN: Duration = Duration::from_millis(300);
/// Floating panel / picker / popover exit duration (`110 ms`).
pub const FLOAT_OUT: Duration = Duration::from_millis(110);
/// Which-key popup opacity fade-in duration (`180 ms`).
pub const WHICH_KEY_FADE_IN: Duration = Duration::from_millis(180);
/// Which-key subgroup list horizontal slide duration (`180 ms`).
pub const WHICH_KEY_SLIDE: Duration = Duration::from_millis(180);
/// Delay before showing which-key for a new pending prefix (`200 ms`).
pub const WHICH_KEY_DELAY: Duration = Duration::from_millis(200);
/// Which-key entrance vertical offset from bottom corner (`10 px`).
pub const WHICH_KEY_IN_Y: f32 = 10.0;
/// Which-key exit vertical offset toward bottom corner (`6 px`).
pub const WHICH_KEY_OUT_Y: f32 = 6.0;
/// Staggered inner content entrance duration (`200 ms`).
pub const FLOAT_CONTENT_IN: Duration = Duration::from_millis(200);
/// Initial delay before floating content settles in (`40 ms`).
pub const FLOAT_CONTENT_DELAY: Duration = Duration::from_millis(40);
/// Per-row stagger interval inside pickers and menus (`12 ms`).
pub const ROW_STAGGER: Duration = Duration::from_millis(12);
/// Maximum number of staggered rows so fast keyboard picks are never slowed.
pub const ROW_STAGGER_CAP: usize = 6;
/// Picker cursor glide duration (`220 ms`).
pub const PICKER_CURSOR: Duration = Duration::from_millis(220);
/// Grid picker selected icon plate lift/scale spring duration (`260 ms`, matching `zendia.css:886`).
pub const PICKER_PLATE_LIFT: Duration = Duration::from_millis(260);
/// Grid picker selected icon plate glow fade duration (`200 ms`, matching `zendia.css:887`).
pub const PICKER_PLATE_GLOW: Duration = Duration::from_millis(200);
/// Picker key badge active color transition (`140 ms`, matching `zendia.css:824`).
pub const PICKER_KEY_FADE: Duration = Duration::from_millis(140);
/// Split-view / window-mode pane header chip entrance duration (`120 ms`, matching `zen-split-view.css:250`).
pub const SPLIT_CHIP_IN: Duration = Duration::from_millis(120);
/// Confirmation card rise duration (`260 ms`).
pub const CONFIRM_RISE: Duration = Duration::from_millis(260);
/// Switch thumb slide duration (`220 ms`).
pub const SWITCH_THUMB: Duration = Duration::from_millis(220);
/// Switch background color duration (`160 ms`).
pub const SWITCH_BG: Duration = Duration::from_millis(160);
/// Effort slider transition duration (`200 ms`).
pub const EFFORT_SLIDE: Duration = Duration::from_millis(200);
/// Statusline repositioning spring (`340 ms`).
pub const STATUSLINE_MOVE: Duration = Duration::from_millis(340);
/// Statusline pointer duck duration (`160 ms`).
pub const STATUSLINE_DUCK: Duration = Duration::from_millis(160);
/// Vim mode window backdrop tint crossfade (`150 ms`, matching `zendia.css:19`).
pub const VIM_MODE_TINT: Duration = Duration::from_millis(150);
/// Mascot per-cell staggered pop-in total window (`280 ms`).
pub const MASCOT_POP_IN: Duration = Duration::from_millis(280);
/// Mascot per-cell staggered pop-out total window (`200 ms`).
pub const MASCOT_POP_OUT: Duration = Duration::from_millis(200);
/// Mascot per-cell staggered shrink/grow spring window (`520 ms`, including `160 ms` whole-scale hold).
pub const MASCOT_SHRINK: Duration = Duration::from_millis(520);
/// Library open/close slide duration (`300 ms`).
pub const LIBRARY_SLIDE: Duration = Duration::from_millis(300);
/// Library section body fade-in duration (`100 ms`).
pub const LIBRARY_SECTION_FADE: Duration = Duration::from_millis(100);
/// Library filter panel slide/fade duration (`280 ms`).
pub const LIBRARY_FILTER_PANEL: Duration = Duration::from_millis(280);
/// Library full-size media preview open/close morph duration (`250 ms`).
pub const MEDIA_PREVIEW_MORPH: Duration = Duration::from_millis(250);
/// Tab switch vertical slide duration (`220 ms`).
pub const TAB_SLIDE: Duration = Duration::from_millis(220);
/// Tab switch opacity fade duration (`140 ms`).
pub const TAB_FADE: Duration = Duration::from_millis(140);
/// Repeat-switch threshold below which tab motion is skipped (`90 ms`).
pub const TAB_REPEAT: Duration = Duration::from_millis(90);
/// Maximum wait for a switched tab to paint before skipping motion (`1500 ms`).
pub const TAB_PENDING: Duration = Duration::from_millis(1500);
/// Dock slide keep-hover delay (`400 ms`).
pub const DOCK_KEEP_HOVER: Duration = Duration::from_millis(400);
/// Dock handle hover activation delay (`100 ms`).
pub const DOCK_HANDLE_HOVER: Duration = Duration::from_millis(100);
/// Dock panel entrance slide duration (`360 ms`, spring).
pub const DOCK_OPEN: Duration = Duration::from_millis(360);
/// Dock panel exit slide duration (`180 ms`, ease-in).
pub const DOCK_CLOSE: Duration = Duration::from_millis(180);
/// Dock panel focus ring / resize grip / slide handle transition (`150 ms`).
pub const DOCK_UI_TRANSITION: Duration = Duration::from_millis(150);
/// Dock closed pill entrance duration (`300 ms`, spring).
pub const DOCK_PILL_IN: Duration = Duration::from_millis(300);
/// Dock closed pill exit duration (`110 ms`, ease-in).
pub const DOCK_PILL_OUT: Duration = Duration::from_millis(110);
/// Compact mode sidebar reveal spring duration (`250 ms`).
pub const COMPACT_SIDEBAR_OPEN: Duration = Duration::from_millis(250);
/// Compact mode sidebar hide ease-out duration (`150 ms`).
pub const COMPACT_SIDEBAR_HIDE: Duration = Duration::from_millis(150);
/// Compact mode top toolbar height/opacity transition (`150 ms`).
pub const COMPACT_TOOLBAR_TOGGLE: Duration = Duration::from_millis(150);
/// Compact mode sidebar keep-hover delay before hiding (`150 ms`).
pub const COMPACT_SIDEBAR_KEEP_HOVER: Duration = Duration::from_millis(150);
/// Compact mode outside-window edge-leave fallback duration on Linux (`1000 ms`).
pub const COMPACT_WINDOW_LEAVE_HIDE: Duration = Duration::from_millis(1000);
/// Compact mode tab-switch sidebar flash duration (`800 ms`).
pub const COMPACT_FLASH_DURATION: Duration = Duration::from_millis(800);
/// Compact mode enter/exit margin collapse duration (`100 ms`).
pub const COMPACT_MODE_MARGIN: Duration = Duration::from_millis(100);
/// Sidebar resize splitter hover reveal delay (`200 ms`).
pub const SIDEBAR_SPLITTER_DELAY: Duration = Duration::from_millis(200);
/// Sidebar resize splitter fade-in duration (`200 ms`).
pub const SIDEBAR_SPLITTER_FADE: Duration = Duration::from_millis(200);
/// Agent working ring active run (`3900 ms`).
pub const AGENT_RING_RUN: Duration = Duration::from_millis(3900);
/// Agent working ring rest interval (`900 ms`).
pub const AGENT_RING_REST: Duration = Duration::from_millis(900);
/// Agent rolling word per-glyph duration (`550 ms`).
pub const AGENT_ROLL: Duration = Duration::from_millis(550);
/// Agent rolling word width glide (`200 ms`).
pub const AGENT_ROLL_GLIDE: Duration = Duration::from_millis(200);
/// Agent streaming veil duration bounds (`120..=400 ms`).
pub const AGENT_VEIL_MIN: Duration = Duration::from_millis(120);
pub const AGENT_VEIL_MAX: Duration = Duration::from_millis(400);

/// The library icon playing forward under the pointer (`350 ms`) and back when it leaves (`300 ms`).
pub const LIBRARY_SPRITE_PLAY: Duration = Duration::from_millis(350);
pub const LIBRARY_SPRITE_REWIND: Duration = Duration::from_millis(300);

/// One turn of a busy spinner (`800 ms`).
pub const SPINNER_TURN: Duration = Duration::from_millis(800);
/// One pass of an indeterminate loading bar (`1100 ms`).
pub const LOADING_BAR_PASS: Duration = Duration::from_millis(1100);
/// Waiting emblem pulse half-cycle (`700 ms`, matching fork's `1.4 s` full cycle).
pub const WAITING_PULSE: Duration = Duration::from_millis(700);
/// Waiting emblem fade-out once loaded (`220 ms`).
pub const WAITING_FADE_OUT: Duration = Duration::from_millis(220);
/// About dialog "Copied" version badge auto-clear delay (`1500 ms`, matching `zen-about.js:24`).
pub const ABOUT_COPIED_HINT: Duration = Duration::from_millis(1500);
/// About dialog minimum checking-for-updates spinner display (`1500 ms`, matching `zen-about.js:23`).
pub const ABOUT_CHECKING_MIN: Duration = Duration::from_millis(1500);
/// Sidebar bottom `+` button rotation duration while Create New popup opens/closes (`200 ms`, matching `ZenUIManager.mjs:659`).
pub const CREATE_NEW_ROTATE: Duration = Duration::from_millis(200);
/// App menu submenu hover-intent grace delay before switching away (`180 ms`).
pub const SUBMENU_HOVER_INTENT: Duration = Duration::from_millis(180);
/// Subtle context menu entrance fade (`90 ms`).
pub const CONTEXT_MENU_FADE: Duration = Duration::from_millis(90);
/// Tab open enter animation (`120 ms`, matching `ZenVerticalTabsManager.mjs:185`).
pub const TAB_OPEN: Duration = Duration::from_millis(120);
/// Tab close exit animation (`100 ms`, matching `ZenVerticalTabsManager.mjs:227`).
pub const TAB_CLOSE: Duration = Duration::from_millis(100);
/// Pinned tab reset/unload `—` to `×` morph (`120 ms`, matching `vertical-tabs.css:674`).
pub const PINNED_CLOSE_MORPH: Duration = Duration::from_millis(120);
/// Pinned separator `"Clear"` fade and icon slide (`150 ms`, matching `vertical-tabs.css:209`).
pub const PINNED_CLEAR_FADE: Duration = Duration::from_millis(150);
/// Tab rename commit confirmation pulse (`250 ms`, matching `ZenTabRenamer.mjs:132`).
pub const TAB_RENAME_PULSE: Duration = Duration::from_millis(250);
/// Folder front flap open/close skew animation (`300 ms`, matching `zen-folder-icon.inc.css:5`).
pub const FOLDER_FLAP: Duration = Duration::from_millis(300);
/// Space switcher icon opacity/scale transition (`180 ms`, matching `zen-workspaces.css:76`).
pub const SPACE_ICON_TRANSITION: Duration = Duration::from_millis(180);
/// Space switch tab strip spring slide (`250 ms`, matching `ZenSpaceManager.mjs:2326`).
pub const SPACE_SWITCH_SLIDE: Duration = Duration::from_millis(250);
/// Sidebar notification entrance/exit slide (`220 ms`).
pub const SIDEBAR_NOTIF_SLIDE: Duration = Duration::from_millis(220);
/// Gradient generator dot spring (`400 ms`, matching `ZenGradientGenerator.mjs:1093`).
pub const GRADIENT_DOT_SPRING: Duration = Duration::from_millis(400);
/// Gradient generator dot hover scale (`200 ms`, matching `zen-gradient-generator.css:281`).
pub const GRADIENT_DOT_HOVER: Duration = Duration::from_millis(200);
/// Add-space swipe `+` icon threshold jump (`260 ms`, matching `zen-animations.css:7`).
pub const SWIPE_ADD_JUMP: Duration = Duration::from_millis(260);
/// Space swipe release settle spring (`220 ms`).
pub const SWIPE_SETTLE: Duration = Duration::from_millis(220);

/// Floating address bar search-mode scale bounce (`250 ms`).
pub const URLBAR_MODE_BOUNCE: Duration = Duration::from_millis(250);
/// Floating address bar search-mode outer glow ramp (`1000 ms`).
pub const URLBAR_MODE_GLOW: Duration = Duration::from_millis(1000);
/// Toast spring pop-in duration (`500 ms`).
pub const TOAST_POP_IN: Duration = Duration::from_millis(500);
/// Toast exit fade/scale duration (`200 ms`).
pub const TOAST_EXIT: Duration = Duration::from_millis(200);
/// Glance open spring duration (`420 ms`).
pub const GLANCE_OPEN: Duration = Duration::from_millis(420);
/// Glance close ease-out duration (`220 ms`).
pub const GLANCE_CLOSE: Duration = Duration::from_millis(220);
/// Glance content fade-out duration (`150 ms`).
pub const GLANCE_CONTENT_FADE: Duration = Duration::from_millis(150);
/// Glance action buttons delayed entrance duration (`200 ms`).
pub const GLANCE_BUTTON_IN: Duration = Duration::from_millis(200);
/// Glance action buttons exit fade duration (`120 ms`).
pub const GLANCE_BUTTON_OUT: Duration = Duration::from_millis(120);
/// Split pane top nub fade duration (`120 ms`).
pub const SPLIT_NUB_FADE: Duration = Duration::from_millis(120);
/// Split pane header chip hover-reveal delay (`150 ms`).
pub const SPLIT_CHIP_DELAY: Duration = Duration::from_millis(150);
/// Split pane layout transition duration (`90 ms`).
pub const SPLIT_LAYOUT: Duration = Duration::from_millis(90);
/// Split drop-zone highlight transition duration (`80 ms`).
pub const SPLIT_DROP_ZONE: Duration = Duration::from_millis(80);
/// Split resize handle hover fade duration (`100 ms`).
pub const SPLIT_HANDLE_FADE: Duration = Duration::from_millis(100);
/// Media controls hover expand duration (`300 ms`).
pub const MEDIA_EXPAND: Duration = Duration::from_millis(300);
/// Media controls exit sink/fade duration (`300 ms`).
pub const MEDIA_EXIT: Duration = Duration::from_millis(300);
/// Media controls reappear delay after switching away from playing tab (`500 ms`).
pub const MEDIA_REAPPEAR_DELAY: Duration = Duration::from_millis(500);
/// Media seek bar thumb scale transition (`150 ms`).
pub const MEDIA_THUMB: Duration = Duration::from_millis(150);
/// Site data panel tile hover/active scale transition (`120 ms`).
pub const SITE_TILE_SCALE: Duration = Duration::from_millis(120);

/// Onboarding Start hero child fade-up duration (`550 ms`).
pub const ONB_START_FADE: Duration = Duration::from_millis(550);
/// Onboarding Start hero stagger interval (`60 ms`: `0/60/120/180 ms`).
pub const ONB_START_STAGGER: Duration = Duration::from_millis(60);
/// Onboarding top rail fade-down duration (`420 ms`).
pub const ONB_RAIL_IN: Duration = Duration::from_millis(420);
/// Onboarding decision deck fade-up duration (`450 ms`).
pub const ONB_DECK_IN: Duration = Duration::from_millis(450);
/// Onboarding chapter text child slide/fade duration (`360 ms`).
pub const ONB_TEXT_ENTER: Duration = Duration::from_millis(360);
/// Onboarding chapter text child stagger interval (`40 ms`: `0/40/80 ms`).
pub const ONB_TEXT_STAGGER: Duration = Duration::from_millis(40);
/// Onboarding chapter page choices slide/fade duration (`380 ms`).
pub const ONB_PAGE_ENTER: Duration = Duration::from_millis(380);
/// Onboarding 2D stage scene scale/fade enter duration (`440 ms`).
pub const ONB_SCENE_ENTER: Duration = Duration::from_millis(440);
/// Onboarding pinned app tile spring pop duration (`450 ms`).
pub const ONB_TILE_POP: Duration = Duration::from_millis(450);
/// Onboarding exit fade duration (`280 ms`).
pub const ONB_EXIT_FADE: Duration = Duration::from_millis(280);
/// Onboarding button hover lift spring duration (`350 ms`).
pub const ONB_BTN_HOVER: Duration = Duration::from_millis(350);
/// Onboarding button active press spring duration (`220 ms`).
pub const ONB_BTN_PRESS: Duration = Duration::from_millis(220);
/// Onboarding choice/pill hover transition duration (`200 ms`).
pub const ONB_CTRL_HOVER: Duration = Duration::from_millis(200);
/// Onboarding icon button hover scale spring duration (`250 ms`).
pub const ONB_ICON_HOVER: Duration = Duration::from_millis(250);
/// Onboarding ambient field color crossfade (`550 ms`).
pub const ONB_AMBIENT_TRANSITION: Duration = Duration::from_millis(550);
/// Onboarding emblem halo ring color crossfade (`350 ms`).
pub const ONB_HALO_TRANSITION: Duration = Duration::from_millis(350);
/// Onboarding stage backdrop color crossfade (`450 ms`).
pub const ONB_STAGE_TRANSITION: Duration = Duration::from_millis(450);
/// Onboarding interactive 2D scene toggle blend (`320 ms` / `380 ms`).
pub const ONB_SCENE_TOGGLE: Duration = Duration::from_millis(320);
/// Onboarding Move In scene card/browser transition (`380 ms`).
pub const ONB_MOVEIN_TRANSITION: Duration = Duration::from_millis(380);
/// Onboarding Vim caption chip lift spring (`300 ms`).
pub const ONB_VIM_CHIP_LIFT: Duration = Duration::from_millis(300);

/// Vertical slide distance for tab switching (`14 px`).
pub const TAB_SLIDE_PX: f32 = 14.0;
/// Starting opacity for tab switch fade (`0.35`).
pub const TAB_FADE_FROM: f32 = 0.35;
/// Floating entrance vertical offset (`-10 px`).
pub const FLOAT_IN_Y: f32 = -10.0;
/// Floating entrance initial scale (`0.96`).
pub const FLOAT_IN_SCALE: f32 = 0.96;
/// Floating exit vertical offset (`-4 px`).
pub const FLOAT_OUT_Y: f32 = -4.0;
/// Floating exit target scale (`0.98`).
pub const FLOAT_OUT_SCALE: f32 = 0.98;
/// Inner content entrance vertical offset (`4 px`).
pub const CONTENT_IN_Y: f32 = 4.0;
/// Active press scale applied to buttons and clickable rows (`0.98`, matching `zen-buttons.css` and `vertical-tabs.css`).
pub const PRESS_SCALE: f32 = 0.98;

/// Returns the capped per-row stagger delay for `index` (`ROW_STAGGER * min(index, ROW_STAGGER_CAP)`).
pub const fn row_stagger_delay(index: usize) -> Duration {
    let capped = if index > ROW_STAGGER_CAP {
        ROW_STAGGER_CAP
    } else {
        index
    };
    Duration::from_millis(ROW_STAGGER.as_millis() as u64 * capped as u64)
}

/// Sampled spring table matching Zendia's `--zendia-spring` (compact sidebar / dock / picker spring).
const SPRING_TABLE: &[(f32, f32)] = &[
    (0.00, 0.0),
    (0.04, 0.0388),
    (0.08, 0.1313),
    (0.12, 0.2498),
    (0.16, 0.3757),
    (0.20, 0.4970),
    (0.24, 0.6068),
    (0.28, 0.7018),
    (0.32, 0.7810),
    (0.36, 0.8448),
    (0.40, 0.8948),
    (0.44, 0.9327),
    (0.48, 0.9603),
    (0.52, 0.9804),
    (0.56, 0.9940),
    (0.60, 1.0025),
    (0.64, 1.0075),
    (0.68, 1.0101),
    (0.72, 1.0109),
    (0.76, 1.0104),
    (0.84, 1.0084),
    (0.95, 1.0045),
    (1.00, 1.0),
];

/// Evaluates a 1D cubic-bezier curve `cubic-bezier(x1, y1, x2, y2)` at normalized time `x` in `0.0..=1.0`.
pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }

    let sample = |a: f32, b: f32, t: f32| -> f32 {
        let inv = 1.0 - t;
        3.0 * inv * inv * t * a + 3.0 * inv * t * t * b + t * t * t
    };
    let slope = |a: f32, b: f32, t: f32| -> f32 {
        let inv = 1.0 - t;
        3.0 * inv * inv * a + 6.0 * inv * t * (b - a) + 3.0 * t * t * (1.0 - b)
    };

    // Newton-Raphson with bisection fallback for steep curves.
    let mut lo = 0.0_f32;
    let mut hi = 1.0_f32;
    let mut t = x;
    for _ in 0..8 {
        let err = sample(x1, x2, t) - x;
        if err.abs() < 1e-5 {
            break;
        }
        if err > 0.0 {
            hi = t;
        } else {
            lo = t;
        }
        let d = slope(x1, x2, t);
        let next = if d.abs() > 1e-6 { t - err / d } else { 0.5 * (lo + hi) };
        t = if next > lo && next < hi { next } else { 0.5 * (lo + hi) };
    }
    sample(y1, y2, t)
}

/// Zendia's slightly underdamped spring curve (`--zendia-spring`).
pub fn zendia_spring(progress: f32, _ease: Ease) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    for window in SPRING_TABLE.windows(2) {
        let (x0, y0) = window[0];
        let (x1, y1) = window[1];
        if t <= x1 {
            let local = (t - x0) / (x1 - x0);
            return y0 + (y1 - y0) * local;
        }
    }
    1.0
}

/// Zendia's standard ease-out (`cubic-bezier(0.22, 1, 0.36, 1)`).
pub fn zendia_ease_out(progress: f32, _ease: Ease) -> f32 {
    cubic_bezier(0.22, 1.0, 0.36, 1.0, progress)
}

/// Zendia's fast exit ease-in (`cubic-bezier(0.4, 0, 1, 1)`).
pub fn zendia_ease_in(progress: f32, _ease: Ease) -> f32 {
    cubic_bezier(0.4, 0.0, 1.0, 1.0, progress)
}

/// Confirmation card rise curve (`cubic-bezier(0.16, 1, 0.3, 1)`).
pub fn zendia_ease_rise(progress: f32, _ease: Ease) -> f32 {
    cubic_bezier(0.16, 1.0, 0.3, 1.0, progress)
}

/// Switch thumb overshoot curve (`cubic-bezier(0.34, 1.4, 0.64, 1)`).
pub fn zendia_switch_spring(progress: f32, _ease: Ease) -> f32 {
    cubic_bezier(0.34, 1.4, 0.64, 1.0, progress)
}

pub const fn spring_fn() -> Function {
    Function::Fn(zendia_spring)
}

pub const fn ease_out_fn() -> Function {
    Function::Fn(zendia_ease_out)
}

pub const fn ease_in_fn() -> Function {
    Function::Fn(zendia_ease_in)
}

pub const fn ease_rise_fn() -> Function {
    Function::Fn(zendia_ease_rise)
}

pub const fn switch_spring_fn() -> Function {
    Function::Fn(zendia_switch_spring)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_matches_endpoints_and_overshoots_gently() {
        assert_eq!(zendia_spring(0.0, Ease::Out), 0.0);
        assert_eq!(zendia_spring(1.0, Ease::Out), 1.0);
        let peak = zendia_spring(0.72, Ease::Out);
        assert!((peak - 1.0109).abs() < 1e-4);
        assert!(zendia_spring(0.5, Ease::Out) > 0.96);
    }

    #[test]
    fn cubic_bezier_easings_hit_endpoints_and_expected_shapes() {
        assert_eq!(zendia_ease_out(0.0, Ease::Out), 0.0);
        assert_eq!(zendia_ease_out(1.0, Ease::Out), 1.0);
        // Ease-out moves quickly early on (> 0.8 at t=0.5).
        assert!(zendia_ease_out(0.5, Ease::Out) > 0.8);

        assert_eq!(zendia_ease_in(0.0, Ease::In), 0.0);
        assert_eq!(zendia_ease_in(1.0, Ease::In), 1.0);
        // Ease-in accelerates late (< 0.35 at t=0.5).
        assert!(zendia_ease_in(0.5, Ease::In) < 0.35);

        assert_eq!(zendia_ease_rise(0.0, Ease::Out), 0.0);
        assert_eq!(zendia_ease_rise(1.0, Ease::Out), 1.0);

        // Switch spring intentionally overshoots 1.0 mid-flight.
        assert!(zendia_switch_spring(0.6, Ease::Out) > 1.0);
        assert_eq!(zendia_switch_spring(1.0, Ease::Out), 1.0);
    }
}
