/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Delayed floating tooltip with optional shortcut badge, clamped inside the window and painted above page content.

use crate::ui::chrome::menus::AnchorRect;
use crate::ui::kit::popup_presence::{PopupHorizontalAlign, anchor_popup_origin};
use crate::ui::theme::{ResolvedScheme, Rgba, TOOLTIP_OVERLAY_LEVEL, ThemeRole, motion, use_theme};
use freya::animation::*;
use freya::prelude::*;
use std::time::Duration;

/// Splits a trailing parenthesized shortcut hint (e.g. `"Reload (Ctrl+R)"` -> `("Reload", Some("Ctrl+R"))`) when
/// callers pass a combined tooltip string without an explicit `.shortcut(...)`.
pub fn split_tooltip_shortcut(raw: &str) -> (&str, Option<&str>) {
    let trimmed = raw.trim();
    if let Some(open) = trimmed.rfind(" (")
        && trimmed.ends_with(')')
    {
        let label = trimmed[..open].trim_end();
        let inner = &trimmed[open + 2..trimmed.len() - 1];
        let looks_like_shortcut = !inner.is_empty()
            && inner.len() <= 24
            && !label.is_empty()
            && (inner.contains('+')
                || inner.contains('<')
                || inner.starts_with("Ctrl")
                || inner.starts_with("Alt")
                || inner.starts_with("Shift")
                || inner.eq_ignore_ascii_case("Esc")
                || inner.eq_ignore_ascii_case("Tab")
                || (inner.starts_with('F') && inner[1..].chars().all(|c| c.is_ascii_digit())));
        if looks_like_shortcut {
            return (label, Some(inner));
        }
    }
    (trimmed, None)
}

/// Maximum tooltip card width before wrapping onto additional lines.
const TOOLTIP_MAX_WIDTH: f32 = 340.0;

/// Estimates tooltip `(width, height)` for window-edge clamping before layout measurement.
pub fn estimate_tooltip_size(text: &str, shortcut: Option<&str>) -> (f32, f32) {
    let text_w = text.chars().count() as f32 * 6.8;
    let shortcut_w = shortcut.map_or(0.0, |s| s.chars().count() as f32 * 6.4 + 18.0);
    let raw_width = text_w + shortcut_w + 20.0;
    let width = raw_width.clamp(48.0, TOOLTIP_MAX_WIDTH);
    let lines = if shortcut.is_none() && raw_width > TOOLTIP_MAX_WIDTH {
        let usable = (TOOLTIP_MAX_WIDTH - 20.0).max(80.0);
        (text_w / usable).ceil().clamp(1.0, 4.0)
    } else {
        1.0
    };
    let height = 12.0 + lines * 16.0;
    (width, height)
}

/// Wraps a control and displays a delayed floating tooltip (with optional shortcut hint) on pointer hover.
///
/// The tooltip floats at [`TOOLTIP_OVERLAY_LEVEL`], clamps inside window bounds, flips above controls near the bottom
/// edge, hides immediately on pointer leave or click, and stops redrawing once hidden.
#[derive(Clone, PartialEq)]
pub struct KitTooltip {
    text: String,
    shortcut: Option<String>,
    delay: Duration,
    enabled: bool,
    fill_width: bool,
    children: Vec<Element>,
    key: DiffKey,
}

impl KeyExt for KitTooltip {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ChildrenExt for KitTooltip {
    fn children_mut(&mut self) -> &mut Vec<Element> {
        &mut self.children
    }
}

impl KitTooltip {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            shortcut: None,
            delay: motion::TOOLTIP_DELAY,
            enabled: true,
            fill_width: false,
            children: Vec::new(),
            key: DiffKey::None,
        }
    }

    /// Sets an explicit shortcut hint displayed beside the tooltip text (e.g. `"Ctrl+R"`).
    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// Sets an optional shortcut hint.
    pub fn maybe_shortcut(mut self, shortcut: Option<String>) -> Self {
        self.shortcut = shortcut;
        self
    }

    /// Overrides the hover delay before showing (defaults to [`motion::TOOLTIP_DELAY`]).
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Disables showing the tooltip (e.g. while a menu anchored to this button is already open).
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sizes the tooltip wrapper to [`Size::fill()`] horizontally (required when wrapping full-width rows or tiles).
    pub fn fill_width(mut self, fill: bool) -> Self {
        self.fill_width = fill;
        self
    }
}

impl Component for KitTooltip {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut anchor = use_state(|| None::<AnchorRect>);
        let mut shown = use_state(|| false);
        let mut delay_task = use_state(|| None::<TaskHandle>);

        use_drop(move || {
            if let Some(handle) = delay_task.write().take() {
                handle.cancel();
            }
        });

        if !self.enabled && *shown.peek() {
            if let Some(handle) = delay_task.write().take() {
                handle.cancel();
            }
            shown.set(false);
        }

        let visible_target = self.enabled && *shown.read();
        let fade = use_animation_with_dependencies(&visible_target, move |conf, open| {
            conf.on_creation(OnCreation::Finish);
            conf.on_change(OnChange::Rerun);
            let (from, to) = if *open { (0.0, 1.0) } else { (1.0, 0.0) };
            AnimNum::new(from, to)
                .duration(motion::TOOLTIP_FADE)
                .ease(Ease::Out)
                .function(motion::ease_out_fn())
        });

        let opacity = fade.get().value().clamp(0.0, 1.0);
        let delay = self.delay;
        let enabled = self.enabled;

        let (label_text, derived_shortcut) = split_tooltip_shortcut(&self.text);
        let shortcut = self.shortcut.as_deref().or(derived_shortcut);

        let popup = (opacity > 0.01 && !label_text.is_empty()).then(|| {
            (*anchor.read()).map(|rect_anchor| {
                let root_size = *Platform::get().current_window().root_size.read();
                let window_size = (root_size.width.max(320.0), root_size.height.max(240.0));
                let est = estimate_tooltip_size(label_text, shortcut);
                // Prefer centered horizontally over the button when room permits; clamp inside window edges.
                let centered_anchor = AnchorRect {
                    x: rect_anchor.x + (rect_anchor.width - est.0) * 0.5,
                    y: rect_anchor.y,
                    width: est.0,
                    height: rect_anchor.height,
                };
                let (x, y) =
                    anchor_popup_origin(centered_anchor, est, window_size, PopupHorizontalAlign::Start, 6.0, 8.0);

                let (bg, border, text_col, key_bg, key_fg) = match theme.scheme {
                    ResolvedScheme::Dark => (
                        Color::from_argb(246, 26, 28, 38),
                        Rgba::from_rgb_alpha(255, 255, 255, 0.14).to_color(),
                        Color::from_rgb(240, 242, 248),
                        Rgba::from_rgb_alpha(255, 255, 255, 0.10).to_color(),
                        Color::from_argb(200, 210, 216, 232),
                    ),
                    ResolvedScheme::Light => (
                        Color::from_argb(250, 250, 251, 254),
                        Rgba::from_rgb_alpha(0, 0, 0, 0.14).to_color(),
                        Color::from_rgb(24, 26, 36),
                        Rgba::from_rgb_alpha(0, 0, 0, 0.07).to_color(),
                        theme.color(ThemeRole::TextMuted),
                    ),
                };

                let text_el = if shortcut.is_none() && est.1 > 30.0 {
                    label()
                        .text(label_text.to_string())
                        .max_width(Size::px(TOOLTIP_MAX_WIDTH - 18.0))
                        .font_size(11.5)
                        .font_weight(FontWeight::MEDIUM)
                        .color(text_col)
                        .max_lines(4)
                } else {
                    label()
                        .text(label_text.to_string())
                        .font_size(11.5)
                        .font_weight(FontWeight::MEDIUM)
                        .color(text_col)
                        .max_lines(1)
                };

                rect()
                    .layer(Layer::OverlayLevel(TOOLTIP_OVERLAY_LEVEL))
                    .position(Position::new_global().left(x).top(y + (1.0 - opacity) * 3.0))
                    .max_width(Size::px(TOOLTIP_MAX_WIDTH))
                    .interactive(Interactive::No)
                    .opacity(opacity)
                    .padding(Gaps::new(5.0, 9.0, 5.0, 9.0))
                    .spacing(7.0)
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .corner_radius(CornerRadius::new_all(7.0))
                    .background(bg)
                    .border(Border::new().width(1.0).alignment(BorderAlignment::Inner).fill(border))
                    .shadow(theme.shadows.card.to_shadow())
                    .child(text_el)
                    .maybe_child(shortcut.map(|sc| {
                        rect()
                            .padding(Gaps::new(1.0, 5.0, 1.0, 5.0))
                            .corner_radius(CornerRadius::new_all(4.0))
                            .background(key_bg)
                            .child(
                                label()
                                    .text(sc.to_string())
                                    .font_size(10.5)
                                    .font_weight(FontWeight::MEDIUM)
                                    .color(key_fg)
                                    .max_lines(1),
                            )
                    }))
            })
        });

        let mut wrapper = rect()
            .on_sized(move |e: Event<SizedEventData>| anchor.set_if_modified(Some(AnchorRect::from_area(e.area))))
            .on_pointer_over(move |_| {
                if !enabled || *shown.peek() || delay_task.peek().is_some() {
                    return;
                }
                let task = spawn(async move {
                    timer(delay).await;
                    shown.set_if_modified(true);
                });
                delay_task.set(Some(task));
            })
            .on_pointer_out(move |_| {
                if let Some(handle) = delay_task.write().take() {
                    handle.cancel();
                }
                shown.set_if_modified(false);
            })
            .on_pointer_down(move |_| {
                if let Some(handle) = delay_task.write().take() {
                    handle.cancel();
                }
                shown.set_if_modified(false);
            })
            .children(self.children.clone())
            .maybe_child(popup.flatten());
        if self.fill_width {
            wrapper = wrapper.width(Size::fill());
        }
        wrapper
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_tooltip_shortcut_extracts_trailing_shortcut_only() {
        assert_eq!(split_tooltip_shortcut("Reload (Ctrl+R)"), ("Reload", Some("Ctrl+R")));
        assert_eq!(
            split_tooltip_shortcut("All agents (<leader>aA)"),
            ("All agents", Some("<leader>aA"))
        );
        assert_eq!(split_tooltip_shortcut("Close (Esc)"), ("Close", Some("Esc")));
        assert_eq!(
            split_tooltip_shortcut("Your name (optional)"),
            ("Your name (optional)", None)
        );
        assert_eq!(split_tooltip_shortcut("Back"), ("Back", None));
    }

    #[test]
    fn estimate_tooltip_size_wraps_long_descriptions_within_max_width() {
        let (short_w, short_h) = estimate_tooltip_size("Reload Page", Some("F5"));
        assert!(short_w < TOOLTIP_MAX_WIDTH);
        assert_eq!(short_h, 28.0);

        let (long_w, long_h) = estimate_tooltip_size(
            "Side by side with the tab: no room beside this tab, so it floats for now",
            None,
        );
        assert_eq!(long_w, TOOLTIP_MAX_WIDTH);
        assert!(long_h > 28.0);
    }
}
