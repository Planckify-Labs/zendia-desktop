/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Overlay and prompt components: [`Toast`], [`AnchoredPopover`] (through Freya's [`Portal`]), and
//! [`ConfirmationCard`].

use crate::ui::kit::controls::IconButton;
use crate::ui::kit::focus_trap::focus_ring_border;
use crate::ui::kit::icons::Icon;
use crate::ui::kit::interactive::use_interactive_feedback;
use crate::ui::kit::panel::GlassPanel;
use crate::ui::kit::popup_presence::PopupPresence;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ThemeRole, use_theme};
use freya::prelude::*;
use zendia_module_api::ui::{UiPlacement, UiTone};

/// Short-lived notification banner styled to Zendia's theme.
#[derive(Clone, PartialEq)]
pub struct Toast {
    message: String,
    detail: Option<String>,
    tone: UiTone,
    on_dismiss: Option<EventHandler<Event<PressEventData>>>,
    key: DiffKey,
}

impl KeyExt for Toast {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Toast {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            detail: None,
            tone: UiTone::Neutral,
            on_dismiss: None,
            key: DiffKey::None,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn tone(mut self, tone: UiTone) -> Self {
        self.tone = tone;
        self
    }

    pub fn on_dismiss(mut self, handler: impl Into<EventHandler<Event<PressEventData>>>) -> Self {
        self.on_dismiss = Some(handler.into());
        self
    }
}

impl Component for Toast {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let accent_role = match self.tone {
            UiTone::Neutral => ThemeRole::Accent,
            _ => ThemeRole::from_tone(self.tone),
        };
        let icon_name = match self.tone {
            UiTone::Success => "check",
            UiTone::Warning => "warning",
            UiTone::Danger => "security-warning",
            UiTone::Neutral | UiTone::Accent => "info",
        };
        let accent = theme.rgba(accent_role);

        let container = rect()
            .a11y_role(AccessibilityRole::Alert)
            .direction(Direction::Horizontal)
            .cross_align(Alignment::center())
            .spacing(theme.spacing.md)
            .min_height(Size::px(42.0))
            .padding(Gaps::new(
                theme.spacing.md,
                theme.spacing.xl,
                theme.spacing.md,
                theme.spacing.xl,
            ))
            .corner_radius(CornerRadius::new_all(theme.radii.card))
            .background(theme.color(ThemeRole::SurfaceGlass))
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(accent.with_alpha(0.45).to_color()),
            )
            .shadow(theme.shadows.card.to_shadow())
            .child(Icon::new(icon_name).size(16.0).role(accent_role))
            .child(
                rect()
                    .spacing(theme.spacing.xxs)
                    .child(
                        label()
                            .text(self.message.clone())
                            .font_size(theme.font_sizes.lg)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(theme.color(ThemeRole::Text)),
                    )
                    .maybe_child(self.detail.as_ref().map(|detail| {
                        label()
                            .text(detail.clone())
                            .font_size(theme.font_sizes.sm)
                            .color(theme.color(ThemeRole::TextMuted))
                    })),
            )
            .maybe_child(self.on_dismiss.as_ref().map(|handler| {
                IconButton::new("close", "Dismiss notification")
                    .size(24.0)
                    .icon_size(12.0)
                    .on_press(handler.clone())
            }));

        if theme.glass.blur > 0.0 {
            container.backdrop_blur(theme.glass.model_blur)
        } else {
            container
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Computes the global top-left coordinates for a popover relative to its anchor area.
pub fn compute_popover_origin(anchor: Area, content: Area, placement: UiPlacement, gap: f32) -> (f32, f32) {
    match placement {
        UiPlacement::Top => (
            anchor.min_x() + (anchor.width() - content.width()) * 0.5,
            anchor.min_y() - content.height() - gap,
        ),
        UiPlacement::Bottom => (
            anchor.min_x() + (anchor.width() - content.width()) * 0.5,
            anchor.max_y() + gap,
        ),
        UiPlacement::Left => (
            anchor.min_x() - content.width() - gap,
            anchor.min_y() + (anchor.height() - content.height()) * 0.5,
        ),
        UiPlacement::Right => (
            anchor.max_x() + gap,
            anchor.min_y() + (anchor.height() - content.height()) * 0.5,
        ),
    }
}

/// Computes the global top-left coordinates for a popover relative to its anchor area, flipping to the opposite side
/// when it overflows `viewport` and clamping inside `[margin, viewport - content - margin]`.
pub fn compute_popover_origin_in_viewport(
    anchor: Area,
    content: Area,
    placement: UiPlacement,
    gap: f32,
    viewport: (f32, f32),
    margin: f32,
) -> (f32, f32) {
    let (mut x, mut y) = compute_popover_origin(anchor, content, placement, gap);
    let (vw, vh) = viewport;
    if vw > 0.0 && vh > 0.0 {
        match placement {
            UiPlacement::Bottom if y + content.height() + margin > vh => {
                let flipped_y = anchor.min_y() - content.height() - gap;
                if flipped_y >= margin {
                    y = flipped_y;
                }
            }
            UiPlacement::Top if y < margin => {
                let flipped_y = anchor.max_y() + gap;
                if flipped_y + content.height() + margin <= vh {
                    y = flipped_y;
                }
            }
            UiPlacement::Right if x + content.width() + margin > vw => {
                let flipped_x = anchor.min_x() - content.width() - gap;
                if flipped_x >= margin {
                    x = flipped_x;
                }
            }
            UiPlacement::Left if x < margin => {
                let flipped_x = anchor.max_x() + gap;
                if flipped_x + content.width() + margin <= vw {
                    x = flipped_x;
                }
            }
            _ => {}
        }
        let max_x = (vw - content.width() - margin).max(margin);
        let max_y = (vh - content.height() - margin).max(margin);
        x = x.clamp(margin, max_x);
        y = y.clamp(margin, max_y);
    }
    (x.max(0.0), y.max(0.0))
}

/// Popover anchored to a trigger element and lifted to the overlay layer with enter/exit presence motion.
#[derive(Clone, PartialEq)]
pub struct AnchoredPopover {
    id: String,
    open: bool,
    placement: UiPlacement,
    gap: f32,
    anchor: Element,
    content: Vec<Element>,
    on_dismiss: Option<EventHandler<()>>,
    key: DiffKey,
}

impl KeyExt for AnchoredPopover {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ChildrenExt for AnchoredPopover {
    fn children_mut(&mut self) -> &mut Vec<Element> {
        &mut self.content
    }
}

impl AnchoredPopover {
    pub fn new(id: impl Into<String>, anchor: impl IntoElement) -> Self {
        Self {
            id: id.into(),
            open: false,
            placement: UiPlacement::Bottom,
            gap: 6.0,
            anchor: anchor.into_element(),
            content: Vec::new(),
            on_dismiss: None,
            key: DiffKey::None,
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn placement(mut self, placement: UiPlacement) -> Self {
        self.placement = placement;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn on_dismiss(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_dismiss = Some(handler.into());
        self
    }
}

impl Component for AnchoredPopover {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut anchor_area = use_state(|| None::<Area>);
        let mut content_area = use_state(|| None::<Area>);
        let mut viewport_size = use_state(|| (0.0_f32, 0.0_f32));

        let measured = anchor_area.read().zip(*content_area.read());
        let vp = *viewport_size.read();
        let origin = match measured {
            Some((anchor, content)) => {
                compute_popover_origin_in_viewport(anchor, content, self.placement, self.gap, vp, 8.0)
            }
            None => (0.0, 0.0),
        };

        let on_dismiss_esc = self.on_dismiss.clone();
        let on_dismiss_bg = self.on_dismiss.clone();

        let mut presence = PopupPresence::anchored(self.open && measured.is_some(), origin)
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL));
        if let Some(cb) = on_dismiss_esc {
            presence = presence.on_escape(cb);
        }
        if let Some(cb) = on_dismiss_bg {
            presence = presence.on_backdrop_mouse_down(move |_| cb.call(()));
        }

        let panel = rect()
            .on_press(|e: Event<PressEventData>| e.stop_propagation())
            .on_sized(move |e: Event<SizedEventData>| {
                if e.area.width() > 0.0 && e.area.height() > 0.0 {
                    content_area.set_if_modified(Some(e.area));
                }
            })
            .child(
                GlassPanel::new()
                    .padding(Gaps::new_all(theme.spacing.md))
                    .spacing(theme.spacing.sm)
                    .children(self.content.clone()),
            );

        rect()
            .on_sized(move |e: Event<SizedEventData>| anchor_area.set_if_modified(Some(e.area)))
            .child(self.anchor.clone())
            .maybe_child((self.open && measured.is_none()).then(|| {
                // Hidden 1-frame probe to measure viewport and popover content size before animating in.
                rect()
                    .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
                    .position(Position::new_global().top(0.0).left(0.0))
                    .width(Size::window_percent(100.0))
                    .height(Size::window_percent(100.0))
                    .interactive(Interactive::No)
                    .on_sized(move |e: Event<SizedEventData>| {
                        if e.area.width() > 0.0 && e.area.height() > 0.0 {
                            viewport_size.set_if_modified((e.area.width(), e.area.height()));
                        }
                    })
                    .child(
                        rect()
                            .position(Position::new_global().left(0.0).top(0.0))
                            .opacity(0.0)
                            .on_sized(move |e: Event<SizedEventData>| {
                                if e.area.width() > 0.0 && e.area.height() > 0.0 {
                                    content_area.set_if_modified(Some(e.area));
                                }
                            })
                            .child(
                                GlassPanel::new()
                                    .padding(Gaps::new_all(theme.spacing.md))
                                    .spacing(theme.spacing.sm)
                                    .children(self.content.clone()),
                            ),
                    )
            }))
            .child(presence.child(panel))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Inline confirmation card shared by the command registry, agent panel, and declared module UI.
#[derive(Clone, PartialEq)]
pub struct ConfirmationCard {
    title: String,
    body: String,
    confirm_label: String,
    cancel_label: String,
    danger: bool,
    on_confirm: Option<EventHandler<Event<PressEventData>>>,
    on_cancel: Option<EventHandler<Event<PressEventData>>>,
    key: DiffKey,
}

impl KeyExt for ConfirmationCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl ConfirmationCard {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            confirm_label: "Confirm".to_string(),
            cancel_label: "Cancel".to_string(),
            danger: false,
            on_confirm: None,
            on_cancel: None,
            key: DiffKey::None,
        }
    }

    pub fn confirm_label(mut self, label: impl Into<String>) -> Self {
        self.confirm_label = label.into();
        self
    }

    pub fn cancel_label(mut self, label: impl Into<String>) -> Self {
        self.cancel_label = label.into();
        self
    }

    pub fn danger(mut self, danger: bool) -> Self {
        self.danger = danger;
        self
    }

    pub fn on_confirm(mut self, handler: impl Into<EventHandler<Event<PressEventData>>>) -> Self {
        self.on_confirm = Some(handler.into());
        self
    }

    pub fn on_cancel(mut self, handler: impl Into<EventHandler<Event<PressEventData>>>) -> Self {
        self.on_cancel = Some(handler.into());
        self
    }
}

impl Component for ConfirmationCard {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let accent_role = if self.danger {
            ThemeRole::Danger
        } else {
            ThemeRole::AgentConfirm
        };
        let accent = theme.rgba(accent_role);
        let confirm_fg = if self.danger && !theme.scheme.is_dark() {
            Color::WHITE
        } else {
            theme.color(ThemeRole::TextOnAccent)
        };

        let cancel_button = ConfirmActionButton {
            label: self.cancel_label.clone(),
            filled: false,
            accent_role,
            confirm_fg,
            on_press: self.on_cancel.clone(),
            key: DiffKey::from(&"confirm-cancel"),
        };
        let confirm_button = ConfirmActionButton {
            label: self.confirm_label.clone(),
            filled: true,
            accent_role,
            confirm_fg,
            on_press: self.on_confirm.clone(),
            key: DiffKey::from(&"confirm-ok"),
        };

        rect()
            .a11y_role(AccessibilityRole::AlertDialog)
            .width(Size::fill())
            .padding(Gaps::new(12.0, 14.0, 12.0, 14.0))
            .spacing(theme.spacing.md)
            .corner_radius(CornerRadius::new_all(theme.radii.card))
            .background(accent.with_alpha(0.12).to_color())
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(accent.with_alpha(0.55).to_color()),
            )
            .shadow(
                Shadow::new()
                    .x(0.0)
                    .y(6.0)
                    .blur(18.0)
                    .spread(-12.0)
                    .color(accent.with_alpha(0.55).to_color()),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .spacing(theme.spacing.md)
                    .child(
                        Icon::new(if self.danger { "security-warning" } else { "warning" })
                            .size(16.0)
                            .role(accent_role),
                    )
                    .child(
                        label()
                            .text(self.title.clone())
                            .font_size(theme.font_sizes.lg)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(theme.color(ThemeRole::Text)),
                    ),
            )
            .child(
                label()
                    .text(self.body.clone())
                    .font_size(theme.font_sizes.md)
                    .color(theme.color(ThemeRole::TextMuted)),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::End)
                    .spacing(theme.spacing.md)
                    .child(cancel_button)
                    .child(confirm_button),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct ConfirmActionButton {
    label: String,
    filled: bool,
    accent_role: ThemeRole,
    confirm_fg: Color,
    on_press: Option<EventHandler<Event<PressEventData>>>,
    key: DiffKey,
}

impl KeyExt for ConfirmActionButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for ConfirmActionButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let feedback = use_interactive_feedback(true);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let accent = theme.rgba(self.accent_role);

        let (bg, border_col, fg, pad_h, weight) = if self.filled {
            (
                feedback.background(
                    accent.to_color(),
                    accent.with_alpha(0.88).to_color(),
                    accent.with_alpha(0.76).to_color(),
                ),
                accent.to_color(),
                self.confirm_fg,
                14.0,
                FontWeight::SEMI_BOLD,
            )
        } else {
            (
                feedback.background(
                    Color::TRANSPARENT,
                    theme.color(ThemeRole::Hover),
                    theme.rgba(ThemeRole::Text).with_alpha(0.14).to_color(),
                ),
                theme.color(ThemeRole::Border),
                theme.color(ThemeRole::Text),
                12.0,
                FontWeight::MEDIUM,
            )
        };

        let border = focus_ring_border(
            focus() == Focus::Keyboard,
            &theme,
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(border_col),
        );
        let on_press = self.on_press.clone();
        let on_press_key = self.on_press.clone();

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label.clone())
            .padding(Gaps::new(6.0, pad_h, 6.0, pad_h))
            .corner_radius(CornerRadius::new_all(theme.radii.md))
            .background(bg)
            .border(border)
            .scale(feedback.scale())
            .center()
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(feedback.on_pointer_enter())
            .on_pointer_leave(feedback.on_pointer_leave())
            .on_pointer_down(feedback.on_pointer_down())
            .on_global_pointer_up(feedback.on_pointer_up())
            .on_press(move |e| {
                if let Some(handler) = &on_press {
                    handler.call(e);
                }
            })
            .on_key_down(move |e: Event<KeyboardEventData>| {
                if (e.key == Key::Named(NamedKey::Enter) || matches!(&e.key, Key::Character(c) if c == " "))
                    && on_press_key.is_some()
                {
                    e.stop_propagation();
                }
            })
            .child(
                label()
                    .text(self.label.clone())
                    .font_size(theme.font_sizes.sm)
                    .font_weight(weight)
                    .color(fg),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use freya::torin::geometry::Point2D;

    #[test]
    fn popover_origin_places_around_anchor_edges() {
        let anchor = Area::new(Point2D::new(100.0, 50.0), Size2D::new(80.0, 30.0));
        let content = Area::new(Point2D::new(0.0, 0.0), Size2D::new(120.0, 60.0));

        assert_eq!(
            compute_popover_origin(anchor, content, UiPlacement::Bottom, 6.0),
            (80.0, 86.0)
        );
        assert_eq!(
            compute_popover_origin(anchor, content, UiPlacement::Top, 6.0),
            (80.0, -16.0)
        );
        assert_eq!(
            compute_popover_origin(anchor, content, UiPlacement::Right, 6.0),
            (186.0, 35.0)
        );
        assert_eq!(
            compute_popover_origin(anchor, content, UiPlacement::Left, 6.0),
            (-26.0, 35.0)
        );
    }

    #[test]
    fn popover_origin_in_viewport_flips_and_clamps_to_edges() {
        let content = Area::new(Point2D::new(0.0, 0.0), Size2D::new(200.0, 100.0));
        // Anchor near bottom-right corner of 400x300 viewport -> flips above and clamps left.
        let near_bottom_right = Area::new(Point2D::new(340.0, 260.0), Size2D::new(40.0, 28.0));
        let (x, y) = compute_popover_origin_in_viewport(
            near_bottom_right,
            content,
            UiPlacement::Bottom,
            6.0,
            (400.0, 300.0),
            8.0,
        );
        assert_eq!(x, 192.0);
        assert_eq!(y, 154.0);

        // Anchor near top-left corner placing Top -> flips below and clamps to margin 8.0.
        let near_top_left = Area::new(Point2D::new(4.0, 12.0), Size2D::new(40.0, 24.0));
        let (x2, y2) =
            compute_popover_origin_in_viewport(near_top_left, content, UiPlacement::Top, 6.0, (400.0, 300.0), 8.0);
        assert_eq!(x2, 8.0);
        assert_eq!(y2, 42.0);
    }
}
