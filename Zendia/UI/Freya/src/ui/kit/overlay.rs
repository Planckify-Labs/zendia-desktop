/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Overlay and prompt components: [`Toast`], [`AnchoredPopover`] (through Freya's [`Portal`]), and
//! [`ConfirmationCard`].

use crate::ui::kit::controls::IconButton;
use crate::ui::kit::icons::Icon;
use crate::ui::kit::panel::GlassPanel;
use crate::ui::theme::{ThemeRole, motion, use_theme};
use freya::animation::Ease;
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

/// Popover anchored to a trigger element and lifted to the overlay layer via Freya's [`Portal`].
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

        let measured = anchor_area.read().zip(*content_area.read());

        let on_dismiss_key = self.on_dismiss.clone();
        let on_dismiss_backdrop = self.on_dismiss.clone();

        rect()
            .on_sized(move |e: Event<SizedEventData>| anchor_area.set_if_modified(Some(e.area)))
            .child(self.anchor.clone())
            .maybe_child(self.open.then(|| {
                rect()
                    .layer(Layer::Overlay)
                    .position(Position::new_global().top(0.0).left(0.0))
                    .width(Size::window_percent(100.0))
                    .height(Size::window_percent(100.0))
                    .on_global_key_down(move |e: Event<KeyboardEventData>| {
                        if e.key == Key::Named(NamedKey::Escape)
                            && let Some(handler) = &on_dismiss_key
                        {
                            handler.call(());
                        }
                    })
                    .child(
                        rect()
                            .position(Position::new_global().top(0.0).left(0.0))
                            .width(Size::window_percent(100.0))
                            .height(Size::window_percent(100.0))
                            .on_press(move |_| {
                                if let Some(handler) = &on_dismiss_backdrop {
                                    handler.call(());
                                }
                            }),
                    )
                    .child(match measured {
                        Some((anchor, content)) => {
                            let (x, y) = compute_popover_origin(anchor, content, self.placement, self.gap);
                            rect()
                                .position(Position::new_global().left(x.max(0.0)).top(y.max(0.0)))
                                .child(
                                    Portal::new(self.id.clone())
                                        .width(Size::px(content.width()))
                                        .height(Size::px(content.height()))
                                        .show(self.open)
                                        .duration(motion::FLOAT_IN)
                                        .ease(Ease::Out)
                                        .function(motion::spring_fn())
                                        .child(
                                            rect().on_press(|e: Event<PressEventData>| e.stop_propagation()).child(
                                                GlassPanel::new()
                                                    .width(Size::fill())
                                                    .padding(Gaps::new_all(theme.spacing.md))
                                                    .spacing(theme.spacing.sm)
                                                    .children(self.content.clone()),
                                            ),
                                        ),
                                )
                                .into_element()
                        }
                        None => rect()
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
                            )
                            .into_element(),
                    })
            }))
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

        let mut cancel_button = Button::new()
            .outline()
            .compact()
            .background(Color::TRANSPARENT)
            .hover_background(theme.color(ThemeRole::Hover))
            .border_fill(theme.color(ThemeRole::Border))
            .focus_border_fill(theme.color(ThemeRole::FocusRing))
            .color(theme.color(ThemeRole::Text))
            .corner_radius(CornerRadius::new_all(theme.radii.md))
            .padding(Gaps::new(6.0, 12.0, 6.0, 12.0))
            .cursor(CursorIcon::Pointer)
            .child(
                label()
                    .text(self.cancel_label.clone())
                    .font_size(theme.font_sizes.sm)
                    .font_weight(FontWeight::MEDIUM),
            );
        if let Some(on_cancel) = &self.on_cancel {
            cancel_button = cancel_button.on_press(on_cancel.clone());
        }

        let mut confirm_button = Button::new()
            .filled()
            .compact()
            .background(accent.to_color())
            .hover_background(accent.with_alpha(0.88).to_color())
            .border_fill(accent.to_color())
            .focus_border_fill(theme.color(ThemeRole::FocusRing))
            .color(confirm_fg)
            .corner_radius(CornerRadius::new_all(theme.radii.md))
            .padding(Gaps::new(6.0, 14.0, 6.0, 14.0))
            .cursor(CursorIcon::Pointer)
            .child(
                label()
                    .text(self.confirm_label.clone())
                    .font_size(theme.font_sizes.sm)
                    .font_weight(FontWeight::SEMI_BOLD),
            );
        if let Some(on_confirm) = &self.on_confirm {
            confirm_button = confirm_button.on_press(on_confirm.clone());
        }

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
}
