/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Gradient generator popup (`#PanelUI-zen-gradient-generator`): dotted color pad with draggable dots (add, move,
//! remove), Auto/Light/Dark scheme toggles, paged preset swatches, interpolated sine-wave opacity slider, and rotary
//! texture dial with live window preview.

use crate::ui::chrome::menus::state::clamp_popup_origin;
use crate::ui::core_view::use_core;
use crate::ui::kit::Icon;
use crate::ui::spaces::state::{SpaceEditingState, try_use_space_editing_state};
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, Rgba, ThemeRole, use_theme};
use freya::engine::prelude::{
    Canvas, Paint, PaintCap, PaintJoin, PaintStyle, PathBuilder, RRect, Rect as SkRect, SkColor, SkPoint,
};
use freya::prelude::*;
use std::f32::consts::PI;
use zendia_browser_core::agent::{cos_f32, sin_f32};
use zendia_browser_core::commands::Command;
use zendia_browser_core::model::SpaceId;
use zendia_browser_core::space_appearance::{
    GRADIENT_PRESET_PAGES, GradientDot, GradientPreset, MAX_GRADIENT_DOTS, MAX_GRADIENT_OPACITY, MIN_GRADIENT_OPACITY,
    PAD_COORD_SIZE, SpaceAppearance, SpaceColorScheme, TEXTURE_DIAL_STEPS, TextureKind, layer_intensity,
    texture_amount_from_dial_point, wave_slider_points,
};

/// Panel width matching Zen's `--panel-width: 364px`.
pub const GRADIENT_PANEL_WIDTH: f32 = 364.0;
const PANEL_PADDING: f32 = 10.0;
const PAD_DISPLAY_SIZE: f32 = GRADIENT_PANEL_WIDTH - PANEL_PADDING * 2.0; // 344px
const DIAL_SIZE: f32 = 76.0;

/// Window-root overlay rendering [`GradientGeneratorCard`] when `SpaceEditingState::theme_picker_open` is true.
#[derive(Clone, PartialEq)]
pub struct GradientGeneratorOverlay;

impl Component for GradientGeneratorOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let editing = try_use_space_editing_state();
        let is_open = editing.is_some_and(|s| *s.theme_picker_open.read());

        if !is_open {
            return rect().into_element();
        }
        let editing = editing.expect("checked above");
        let for_draft = *editing.theme_picker_for_draft.read();
        let target_space_id = *editing.theme_picker_space_id.read();
        let anchor = *editing.theme_picker_anchor.read();

        let (appearance, resolved_space_id) = if for_draft {
            (editing.draft_appearance.read().clone(), None)
        } else {
            let model = core.model().read();
            let sid = target_space_id
                .or(model.active_space)
                .or_else(|| model.spaces.first().map(|s| s.id));
            let app = sid.map(|id| model.space_appearance(id)).unwrap_or_default();
            (app, sid)
        };

        let root_size = *Platform::get().current_window().root_size.read();
        let window_size = (root_size.width.max(400.0), root_size.height.max(540.0));
        let (x, y) = clamp_popup_origin(anchor, (GRADIENT_PANEL_WIDTH, 496.0), window_size, 12.0);

        let esc_core = core.clone();
        let scrim_core = core.clone();

        rect()
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if matches!(e.key, Key::Named(NamedKey::Escape)) {
                    e.stop_propagation();
                    esc_core.invoke_or_log(Command::ThemePickerClose);
                }
            })
            .child(
                rect()
                    .position(Position::new_global().left(0.0).top(0.0))
                    .width(Size::window_percent(100.0))
                    .height(Size::window_percent(100.0))
                    .on_mouse_down(move |_| {
                        scrim_core.invoke_or_log(Command::ThemePickerClose);
                    }),
            )
            .child(
                rect()
                    .position(Position::new_global().left(x).top(y))
                    .child(GradientGeneratorCard {
                        appearance,
                        space_id: resolved_space_id,
                        for_draft,
                        editing: Some(editing),
                        key: DiffKey::from(&"gradient-generator-popup"),
                    }),
            )
            .into_element()
    }
}

/// Self-contained gradient generator panel rendered both inside [`GradientGeneratorOverlay`] and inline in the gallery.
#[derive(Clone, PartialEq)]
pub struct GradientGeneratorCard {
    pub appearance: SpaceAppearance,
    pub space_id: Option<SpaceId>,
    pub for_draft: bool,
    pub editing: Option<SpaceEditingState>,
    pub key: DiffKey,
}

impl KeyExt for GradientGeneratorCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl GradientGeneratorCard {
    pub fn standalone(appearance: SpaceAppearance, space_id: Option<SpaceId>) -> Self {
        Self {
            appearance,
            space_id,
            for_draft: false,
            editing: None,
            key: DiffKey::None,
        }
    }
}

impl Component for GradientGeneratorCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();
        let mut pad_area = use_state(|| (0.0f32, 0.0f32, PAD_DISPLAY_SIZE, PAD_DISPLAY_SIZE));
        let mut dragging_primary = use_state(|| false);
        let mut wave_area = use_state(|| (0.0f32, 0.0f32, 220.0f32, 44.0f32));
        let mut dragging_wave = use_state(|| false);
        let mut dial_area = use_state(|| (0.0f32, 0.0f32, DIAL_SIZE, DIAL_SIZE));
        let mut dragging_dial = use_state(|| false);
        let mut dial_dragged = use_state(|| false);
        let mut local_page = use_state(|| 0usize);

        let appearance = self.appearance.clone();
        let space_id = self.space_id;
        let for_draft = self.for_draft;
        let editing = self.editing;

        let active_page = editing
            .map(|e| *e.preset_page.read())
            .unwrap_or_else(|| *local_page.read())
            .min(GRADIENT_PRESET_PAGES.len().saturating_sub(1));

        let commit_appearance = {
            let core = core.clone();
            move |next: SpaceAppearance| {
                if for_draft && let Some(ed) = editing {
                    let mut draft = ed.draft_appearance;
                    draft.set(next);
                } else {
                    core.invoke_or_log(Command::SpaceAppearanceSet {
                        space_id,
                        appearance: next,
                    });
                }
            }
        };

        let is_dark = theme.scheme == ResolvedScheme::Dark;
        let (panel_bg, panel_border, pad_bg, dot_grid_sk, text_color, muted_icon) = if is_dark {
            (
                Color::from_rgb(30, 30, 32),
                Rgba::from_rgb_alpha(255, 255, 255, 0.12).to_color(),
                Color::from_rgb(38, 38, 41),
                SkColor::from_argb(26, 255, 255, 255),
                Color::from_argb(240, 255, 255, 255),
                Color::from_argb(175, 255, 255, 255),
            )
        } else {
            (
                Color::from_rgb(242, 242, 244),
                Rgba::from_rgb_alpha(0, 0, 0, 0.14).to_color(),
                Color::from_rgb(225, 225, 228),
                SkColor::from_argb(36, 0, 0, 0),
                Color::from_argb(235, 20, 22, 28),
                Color::from_argb(170, 20, 22, 28),
            )
        };

        let current_scheme_btn = match appearance.scheme {
            SpaceColorScheme::Auto => {
                // Show the active resolved scheme highlight when window is explicitly light/dark or Auto when System.
                match core.model().read().theme.scheme {
                    zendia_browser_core::model::ThemeScheme::System => SpaceColorScheme::Auto,
                    zendia_browser_core::model::ThemeScheme::Light => SpaceColorScheme::Light,
                    zendia_browser_core::model::ThemeScheme::Dark => SpaceColorScheme::Dark,
                }
            }
            explicit => explicit,
        };

        let dot_grid_cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_pad_dot_grid(canvas, ctx.size.width, ctx.size.height, dot_grid_sk);
            }
        });

        // Scheme toggle bar (Auto / Light / Dark) at top of color pad.
        let scheme_bar = rect()
            .position(Position::new_absolute().top(14.0).left(0.0))
            .width(Size::percent(100.0))
            .direction(Direction::Horizontal)
            .main_align(Alignment::center())
            .spacing(6.0)
            .children(SpaceColorScheme::ALL.into_iter().map(|scheme_opt| {
                let icon = match scheme_opt {
                    SpaceColorScheme::Auto => "sparkles",
                    SpaceColorScheme::Light => "sun",
                    SpaceColorScheme::Dark => "moon",
                };
                let scheme_core = core.clone();
                let scheme_app = appearance.clone();
                let commit = commit_appearance.clone();
                PadActionButton {
                    icon: icon.to_string(),
                    alt: format!("{} color scheme", scheme_opt.label()),
                    active: current_scheme_btn == scheme_opt,
                    disabled: false,
                    custom_harmony_icon: false,
                    on_press: EventHandler::new(move |_| {
                        if for_draft {
                            let mut next = scheme_app.clone();
                            next.scheme = scheme_opt;
                            commit(next);
                            scheme_core.invoke_or_log(Command::ThemeSet {
                                scheme: Some(scheme_opt.to_theme_scheme()),
                                overrides: None,
                            });
                        } else {
                            scheme_core.invoke_or_log(Command::SpaceAppearanceSetScheme {
                                space_id,
                                scheme: scheme_opt,
                            });
                        }
                    }),
                    key: DiffKey::from(&scheme_opt.as_str()),
                }
                .into_element()
            }));

        // Color action bar (+, -, harmony) at bottom of color pad.
        let dot_count = appearance.dots.len();
        let add_app = appearance.clone();
        let add_commit = commit_appearance.clone();
        let remove_app = appearance.clone();
        let remove_commit = commit_appearance.clone();
        let harmony_app = appearance.clone();
        let harmony_commit = commit_appearance.clone();
        let harmony_core = core.clone();

        let actions_bar = rect()
            .position(Position::new_absolute().bottom(12.0).left(0.0))
            .width(Size::percent(100.0))
            .direction(Direction::Horizontal)
            .main_align(Alignment::center())
            .spacing(6.0)
            .child(PadActionButton {
                icon: "plus".to_string(),
                alt: "Add color dot".to_string(),
                active: false,
                disabled: dot_count == 0 || dot_count >= MAX_GRADIENT_DOTS,
                custom_harmony_icon: false,
                on_press: EventHandler::new(move |_| {
                    let mut next = add_app.clone();
                    next.add_dot(None);
                    add_commit(next);
                }),
                key: DiffKey::from(&"grad-add"),
            })
            .child(PadActionButton {
                icon: "zoom-out".to_string(),
                alt: "Remove color dot".to_string(),
                active: false,
                disabled: dot_count == 0,
                custom_harmony_icon: false,
                on_press: EventHandler::new(move |_| {
                    let mut next = remove_app.clone();
                    next.remove_dot(None);
                    remove_commit(next);
                }),
                key: DiffKey::from(&"grad-remove"),
            })
            .child(PadActionButton {
                icon: "palette".to_string(),
                alt: format!("Cycle color harmony ({})", appearance.harmony.label()),
                active: false,
                disabled: dot_count < 2,
                custom_harmony_icon: true,
                on_press: EventHandler::new(move |_| {
                    if for_draft {
                        let mut next = harmony_app.clone();
                        next.cycle_harmony();
                        harmony_commit(next);
                    } else {
                        harmony_core.invoke_or_log(Command::SpaceAppearanceCycleHarmony { space_id });
                    }
                }),
                key: DiffKey::from(&"grad-harmony"),
            });

        // Dots placed on the color pad.
        let dot_elements: Vec<Element> = appearance
            .dots
            .iter()
            .map(|dot| {
                let remove_dot_app = appearance.clone();
                let remove_dot_commit = commit_appearance.clone();
                let dot_id = dot.id;
                PadColorDot {
                    dot: dot.clone(),
                    on_drag_start: EventHandler::new(move |_| {
                        dragging_primary.set(true);
                    }),
                    on_remove: EventHandler::new(move |_| {
                        let mut next = remove_dot_app.clone();
                        next.remove_dot(Some(dot_id));
                        remove_dot_commit(next);
                    }),
                    key: DiffKey::from(&dot.id),
                }
                .into_element()
            })
            .collect();

        let pad_click_app = appearance.clone();
        let pad_click_commit = commit_appearance.clone();
        let pad_move_app = appearance.clone();
        let pad_move_commit = commit_appearance.clone();

        let color_pad = rect()
            .a11y_role(AccessibilityRole::Group)
            .a11y_alt("Gradient color pad")
            .width(Size::px(PAD_DISPLAY_SIZE))
            .height(Size::px(PAD_DISPLAY_SIZE))
            .corner_radius(CornerRadius::new_all(8.0))
            .overflow(Overflow::Clip)
            .background(pad_bg)
            .on_sized(move |e: Event<SizedEventData>| {
                let next = (
                    e.area.min_x(),
                    e.area.min_y(),
                    e.area.width().max(1.0),
                    e.area.height().max(1.0),
                );
                if *pad_area.peek() != next {
                    pad_area.set(next);
                }
            })
            .on_pointer_down(move |e: Event<PointerEventData>| {
                if e.button() != Some(MouseButton::Left) {
                    return;
                }
                e.stop_propagation();
                let (x0, y0, w, h) = *pad_area.peek();
                let local_x = (e.global_location().x as f32 - x0).clamp(0.0, w);
                let local_y = (e.global_location().y as f32 - y0).clamp(0.0, h);
                // Ignore clicks on the top/bottom button strips.
                if local_y < 48.0 || local_y > h - 46.0 {
                    return;
                }
                let coord_x = (local_x / w) * PAD_COORD_SIZE;
                let coord_y = (local_y / h) * PAD_COORD_SIZE;
                let mut next = pad_click_app.clone();
                if next.dots.is_empty() {
                    next.add_dot(Some((coord_x, coord_y)));
                } else {
                    next.move_primary_dot(coord_x, coord_y);
                }
                pad_click_commit(next);
                dragging_primary.set(true);
            })
            .on_global_pointer_move(move |e: Event<PointerEventData>| {
                if !*dragging_primary.peek() {
                    return;
                }
                let (x0, y0, w, h) = *pad_area.peek();
                let local_x = (e.global_location().x as f32 - x0).clamp(0.0, w);
                let local_y = (e.global_location().y as f32 - y0).clamp(0.0, h);
                let coord_x = (local_x / w) * PAD_COORD_SIZE;
                let coord_y = (local_y / h) * PAD_COORD_SIZE;
                let mut next = pad_move_app.clone();
                next.move_primary_dot(coord_x, coord_y);
                pad_move_commit(next);
            })
            .on_global_pointer_up(move |_| {
                if *dragging_primary.peek() {
                    dragging_primary.set(false);
                }
            })
            .child(
                rect()
                    .position(Position::new_absolute().left(0.0).top(0.0))
                    .width(Size::percent(100.0))
                    .height(Size::percent(100.0))
                    .background(Fill::Callback(dot_grid_cb)),
            )
            .maybe_child(appearance.dots.is_empty().then(|| {
                rect()
                    .position(Position::new_absolute().left(0.0).top(0.0))
                    .width(Size::percent(100.0))
                    .height(Size::percent(100.0))
                    .center()
                    .child(
                        label()
                            .text("Click to add a color")
                            .font_size(13.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(text_color),
                    )
            }))
            .children(dot_elements)
            .child(scheme_bar)
            .child(actions_bar);

        // Preset color swatches row with < and > arrows.
        let page_count = GRADIENT_PRESET_PAGES.len();
        let current_presets = GRADIENT_PRESET_PAGES[active_page].presets;

        let prev_disabled = active_page == 0;
        let next_disabled = active_page + 1 >= page_count;

        let swatches_row = rect()
            .width(Size::px(PAD_DISPLAY_SIZE))
            .height(Size::px(32.0))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .child(PageArrowButton {
                is_left: true,
                disabled: prev_disabled,
                color: muted_icon,
                on_press: EventHandler::new(move |_| {
                    if active_page > 0 {
                        let next = active_page - 1;
                        if let Some(ed) = editing {
                            let mut p = ed.preset_page;
                            p.set(next);
                        } else {
                            local_page.set(next);
                        }
                    }
                }),
                key: DiffKey::from(&"preset-prev"),
            })
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .padding(Gaps::new(0.0, 4.0, 0.0, 4.0))
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::center())
                    .main_align(Alignment::SpaceBetween)
                    .children(current_presets.iter().enumerate().map(|(idx, preset)| {
                        let is_selected = appearance.preset == Some((active_page, idx));
                        let preset_core = core.clone();
                        let preset_app = appearance.clone();
                        let preset_commit = commit_appearance.clone();
                        PresetSwatch {
                            preset: *preset,
                            selected: is_selected,
                            page: active_page,
                            index: idx,
                            on_press: EventHandler::new(move |_| {
                                if for_draft {
                                    if let Some(mut next) = SpaceAppearance::from_preset(active_page, idx) {
                                        next.opacity = preset_app.opacity;
                                        next.texture_kind = preset_app.texture_kind;
                                        next.texture_amount = preset_app.texture_amount;
                                        next.scheme = preset_app.scheme;
                                        next.profile = preset_app.profile.clone();
                                        preset_commit(next);
                                    }
                                } else {
                                    preset_core.invoke_or_log(Command::SpaceAppearanceApplyPreset {
                                        space_id,
                                        page: active_page,
                                        index: idx,
                                    });
                                }
                            }),
                            key: DiffKey::from(&(active_page * 32 + idx)),
                        }
                        .into_element()
                    })),
            )
            .child(PageArrowButton {
                is_left: false,
                disabled: next_disabled,
                color: muted_icon,
                on_press: EventHandler::new(move |_| {
                    if active_page + 1 < page_count {
                        let next = active_page + 1;
                        if let Some(ed) = editing {
                            let mut p = ed.preset_page;
                            p.set(next);
                        } else {
                            local_page.set(next);
                        }
                    }
                }),
                key: DiffKey::from(&"preset-next"),
            });

        // Wave opacity slider (left) + Texture rotary dial (right).
        let is_default_empty = appearance.dots.is_empty();
        let wave_intensity = if is_default_empty {
            1.0
        } else {
            layer_intensity(appearance.opacity)
        };
        let wave_cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_wave_slider(
                    canvas,
                    ctx.size.width,
                    ctx.size.height,
                    wave_intensity,
                    is_dark,
                    !is_default_empty,
                );
            }
        });

        let update_opacity_from_x = {
            let core = core.clone();
            let app = appearance.clone();
            let commit = commit_appearance.clone();
            move |global_x: f32| {
                let (x0, _, w, _) = *wave_area.peek();
                let frac = ((global_x - x0 - 10.0) / (w - 20.0).max(1.0)).clamp(0.0, 1.0);
                let opacity = MIN_GRADIENT_OPACITY + frac * (MAX_GRADIENT_OPACITY - MIN_GRADIENT_OPACITY);
                if for_draft {
                    let mut next = app.clone();
                    next.set_opacity(opacity);
                    commit(next);
                } else {
                    core.invoke_or_log(Command::SpaceAppearanceSetOpacity { space_id, opacity });
                }
            }
        };
        let wave_down = update_opacity_from_x.clone();
        let wave_move = update_opacity_from_x;

        let wave_slider = rect()
            .a11y_focusable(!is_default_empty)
            .a11y_role(AccessibilityRole::Slider)
            .a11y_alt("Gradient opacity")
            .width(Size::flex(1.0))
            .height(Size::px(50.0))
            .background(Fill::Callback(wave_cb))
            .cursor(if is_default_empty {
                CursorIcon::Default
            } else {
                CursorIcon::Pointer
            })
            .on_sized(move |e: Event<SizedEventData>| {
                let next = (
                    e.area.min_x(),
                    e.area.min_y(),
                    e.area.width().max(1.0),
                    e.area.height().max(1.0),
                );
                if *wave_area.peek() != next {
                    wave_area.set(next);
                }
            })
            .on_pointer_down(move |e: Event<PointerEventData>| {
                if is_default_empty || e.button() != Some(MouseButton::Left) {
                    return;
                }
                e.stop_propagation();
                dragging_wave.set(true);
                wave_down(e.global_location().x as f32);
            })
            .on_global_pointer_move(move |e: Event<PointerEventData>| {
                if *dragging_wave.peek() {
                    wave_move(e.global_location().x as f32);
                }
            })
            .on_global_pointer_up(move |_| {
                if *dragging_wave.peek() {
                    dragging_wave.set(false);
                }
            });

        let tex_kind = appearance.texture_kind;
        let tex_amount = appearance.texture_amount;
        let dial_cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_texture_dial(canvas, ctx.size.width, ctx.size.height, tex_kind, tex_amount, is_dark);
            }
        });

        let dial_move_core = core.clone();
        let dial_move_app = appearance.clone();
        let dial_move_commit = commit_appearance.clone();
        let dial_click_core = core.clone();
        let dial_click_app = appearance.clone();
        let dial_click_commit = commit_appearance.clone();

        let texture_dial = rect()
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Slider)
            .a11y_alt(format!(
                "Texture style {} at {}%",
                tex_kind.label(),
                (tex_amount * 100.0).round() as u32
            ))
            .width(Size::px(DIAL_SIZE))
            .height(Size::px(DIAL_SIZE))
            .background(Fill::Callback(dial_cb))
            .cursor(CursorIcon::Pointer)
            .on_sized(move |e: Event<SizedEventData>| {
                let next = (
                    e.area.min_x(),
                    e.area.min_y(),
                    e.area.width().max(1.0),
                    e.area.height().max(1.0),
                );
                if *dial_area.peek() != next {
                    dial_area.set(next);
                }
            })
            .on_pointer_down(move |e: Event<PointerEventData>| {
                if e.button() != Some(MouseButton::Left) {
                    return;
                }
                e.stop_propagation();
                dragging_dial.set(true);
                dial_dragged.set(false);
            })
            .on_global_pointer_move(move |e: Event<PointerEventData>| {
                if !*dragging_dial.peek() {
                    return;
                }
                dial_dragged.set_if_modified(true);
                let (x0, y0, w, h) = *dial_area.peek();
                let dx = e.global_location().x as f32 - (x0 + w * 0.5);
                let dy = e.global_location().y as f32 - (y0 + h * 0.5);
                let amount = texture_amount_from_dial_point(dx, dy);
                let kind = if amount > 0.0 && tex_kind == TextureKind::None {
                    TextureKind::Grain
                } else {
                    tex_kind
                };
                if for_draft {
                    let mut next = dial_move_app.clone();
                    next.set_texture(kind, amount);
                    dial_move_commit(next);
                } else {
                    dial_move_core.invoke_or_log(Command::SpaceAppearanceSetTexture { space_id, kind, amount });
                }
            })
            .on_global_pointer_up(move |_| {
                if *dragging_dial.peek() {
                    let was_drag = *dial_dragged.peek();
                    dragging_dial.set(false);
                    dial_dragged.set(false);
                    if !was_drag {
                        let mut next = dial_click_app.clone();
                        next.cycle_texture(true);
                        if for_draft {
                            dial_click_commit(next);
                        } else {
                            dial_click_core.invoke_or_log(Command::SpaceAppearanceSetTexture {
                                space_id,
                                kind: next.texture_kind,
                                amount: next.texture_amount,
                            });
                        }
                    }
                }
            });

        let controls_row = rect()
            .width(Size::px(PAD_DISPLAY_SIZE))
            .padding(Gaps::new(2.0, 6.0, 2.0, 6.0))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::center())
            .spacing(14.0)
            .child(wave_slider)
            .child(texture_dial);

        rect()
            .a11y_role(AccessibilityRole::Dialog)
            .a11y_alt("Space theme gradient generator")
            .width(Size::px(GRADIENT_PANEL_WIDTH))
            .padding(Gaps::new_all(PANEL_PADDING))
            .spacing(10.0)
            .corner_radius(CornerRadius::new_all(12.0))
            .background(panel_bg)
            .border(
                Border::new()
                    .width(1.0)
                    .alignment(BorderAlignment::Inner)
                    .fill(panel_border),
            )
            .shadow(
                Shadow::new()
                    .x(0.0)
                    .y(10.0)
                    .blur(28.0)
                    .color(Color::from_argb(110, 0, 0, 0)),
            )
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .child(color_pad)
            .child(swatches_row)
            .child(controls_row)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Subtle procedural grain/sand/denim/tweed overlay painted over the window shell backdrop when a space enables texture.
#[derive(Clone, PartialEq)]
pub struct SpaceTextureOverlay {
    pub kind: TextureKind,
    pub amount: f32,
}

impl Component for SpaceTextureOverlay {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let kind = self.kind;
        let amount = self.amount.clamp(0.0, 1.0);
        if kind == TextureKind::None || amount <= 0.005 {
            return rect().into_element();
        }
        let is_dark = theme.scheme == ResolvedScheme::Dark;
        let cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_backdrop_texture(canvas, ctx.size.width, ctx.size.height, kind, amount, is_dark);
            }
        });
        rect()
            .position(Position::new_absolute().left(0.0).top(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
            .background(Fill::Callback(cb))
            .into_element()
    }
}

#[derive(Clone, PartialEq)]
struct PadActionButton {
    icon: String,
    alt: String,
    active: bool,
    disabled: bool,
    custom_harmony_icon: bool,
    on_press: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for PadActionButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for PadActionButton {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let is_dark = theme.scheme == ResolvedScheme::Dark;
        let disabled = self.disabled;
        let on_press = self.on_press.clone();

        let bg = if self.active {
            if is_dark {
                Rgba::from_rgb_alpha(255, 255, 255, 0.14).to_color()
            } else {
                Rgba::from_rgb_alpha(0, 0, 0, 0.12).to_color()
            }
        } else if hovered() && !disabled {
            if is_dark {
                Rgba::from_rgb_alpha(255, 255, 255, 0.10).to_color()
            } else {
                Rgba::from_rgb_alpha(0, 0, 0, 0.08).to_color()
            }
        } else {
            Color::TRANSPARENT
        };

        let icon_sk = if is_dark {
            SkColor::from_argb(if disabled { 110 } else { 230 }, 255, 255, 255)
        } else {
            SkColor::from_argb(if disabled { 95 } else { 195 }, 20, 22, 28)
        };

        let glyph: Element = if self.custom_harmony_icon {
            let cb = RenderCallback::new(move |ctx| {
                if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                    paint_harmony_icon(canvas, ctx.size.width, ctx.size.height, icon_sk);
                }
            });
            rect()
                .width(Size::px(16.0))
                .height(Size::px(16.0))
                .background(Fill::Callback(cb))
                .into_element()
        } else {
            Icon::new(self.icon.clone())
                .size(15.0)
                .role(ThemeRole::Text)
                .into_element()
        };

        rect()
            .a11y_focusable(!disabled)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.alt.clone())
            .width(Size::px(30.0))
            .height(Size::px(30.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(bg)
            .opacity(if disabled { 0.45 } else { 1.0 })
            .center()
            .cursor(if disabled {
                CursorIcon::Default
            } else {
                CursorIcon::Pointer
            })
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                if !disabled {
                    on_press.call(());
                }
            })
            .child(glyph)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct PadColorDot {
    dot: GradientDot,
    on_drag_start: EventHandler<()>,
    on_remove: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for PadColorDot {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for PadColorDot {
    fn render(&self) -> impl IntoElement {
        let mut hovered = use_state(|| false);
        let size = if self.dot.is_primary { 38.0 } else { 22.0 };
        let border_w = if self.dot.is_primary { 5.5 } else { 3.0 };
        let scale = PAD_DISPLAY_SIZE / PAD_COORD_SIZE;
        let left = (self.dot.x * scale - size * 0.5).clamp(4.0, PAD_DISPLAY_SIZE - size - 4.0);
        let top = (self.dot.y * scale - size * 0.5).clamp(4.0, PAD_DISPLAY_SIZE - size - 4.0);
        let is_primary = self.dot.is_primary;
        let on_drag_start = self.on_drag_start.clone();
        let on_remove = self.on_remove.clone();

        rect()
            .position(Position::new_absolute().left(left).top(top))
            .width(Size::px(if hovered() && is_primary { size + 2.0 } else { size }))
            .height(Size::px(if hovered() && is_primary { size + 2.0 } else { size }))
            .corner_radius(CornerRadius::new_all(999.0))
            .background(Color::from_rgb(self.dot.rgb.0, self.dot.rgb.1, self.dot.rgb.2))
            .border(
                Border::new()
                    .width(border_w)
                    .alignment(BorderAlignment::Inner)
                    .fill(Color::WHITE),
            )
            .shadow(
                Shadow::new()
                    .x(0.0)
                    .y(2.0)
                    .blur(8.0)
                    .color(Color::from_argb(100, 0, 0, 0)),
            )
            .cursor(if is_primary {
                CursorIcon::Grab
            } else {
                CursorIcon::Pointer
            })
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_pointer_down(move |e: Event<PointerEventData>| {
                e.stop_propagation();
                if e.button() == Some(MouseButton::Right) {
                    on_remove.call(());
                } else if e.button() == Some(MouseButton::Left) && is_primary {
                    on_drag_start.call(());
                }
            })
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct PresetSwatch {
    preset: GradientPreset,
    selected: bool,
    page: usize,
    index: usize,
    on_press: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for PresetSwatch {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for PresetSwatch {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let mut hovered = use_state(|| false);
        let preset = self.preset;
        let on_press = self.on_press.clone();

        let swatch_cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_preset_swatch(canvas, ctx.size.width, ctx.size.height, preset);
            }
        });

        let ring_color = if self.selected {
            theme.color(ThemeRole::Accent)
        } else if hovered() {
            Color::from_argb(180, 255, 255, 255)
        } else {
            Color::from_argb(40, 0, 0, 0)
        };

        rect()
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("Color preset page {} item {}", self.page + 1, self.index + 1))
            .width(Size::px(26.0))
            .height(Size::px(26.0))
            .corner_radius(CornerRadius::new_all(999.0))
            .overflow(Overflow::Clip)
            .background(Fill::Callback(swatch_cb))
            .border(
                Border::new()
                    .width(if self.selected { 2.0 } else { 1.0 })
                    .alignment(BorderAlignment::Inner)
                    .fill(ring_color),
            )
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on_press.call(());
            })
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[derive(Clone, PartialEq)]
struct PageArrowButton {
    is_left: bool,
    disabled: bool,
    color: Color,
    on_press: EventHandler<()>,
    key: DiffKey,
}

impl KeyExt for PageArrowButton {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for PageArrowButton {
    fn render(&self) -> impl IntoElement {
        let mut hovered = use_state(|| false);
        let disabled = self.disabled;
        let is_left = self.is_left;
        let sk_color = SkColor::from_argb(
            if disabled { 75 } else { self.color.a() },
            self.color.r(),
            self.color.g(),
            self.color.b(),
        );
        let on_press = self.on_press.clone();

        let cb = RenderCallback::new(move |ctx| {
            if let RenderTarget::Background { canvas, .. } = &mut ctx.target {
                paint_chevron(canvas, ctx.size.width, ctx.size.height, is_left, sk_color);
            }
        });

        rect()
            .a11y_focusable(!disabled)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(if is_left {
                "Previous preset page"
            } else {
                "Next preset page"
            })
            .width(Size::px(24.0))
            .height(Size::px(28.0))
            .corner_radius(CornerRadius::new_all(6.0))
            .background(if hovered() && !disabled {
                Color::from_argb(24, 128, 128, 128)
            } else {
                Color::TRANSPARENT
            })
            .center()
            .cursor(if disabled {
                CursorIcon::Default
            } else {
                CursorIcon::Pointer
            })
            .on_pointer_enter(move |_| hovered.set_if_modified(true))
            .on_pointer_leave(move |_| hovered.set_if_modified(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                if !disabled {
                    on_press.call(());
                }
            })
            .child(
                rect()
                    .width(Size::px(12.0))
                    .height(Size::px(12.0))
                    .background(Fill::Callback(cb)),
            )
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

// ---------------------------------------------------------------------------
// Native Skia painters for pad dot grid, preset swatches, wave slider, dial,
// harmony icon, and backdrop grain overlay
// ---------------------------------------------------------------------------

fn paint_pad_dot_grid(canvas: &Canvas, width: f32, height: f32, color: SkColor) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(PaintStyle::Fill);
    paint.set_color(color);

    let step = 6.0;
    let mut y = 4.0;
    while y < height {
        let mut x = 4.0;
        while x < width {
            canvas.draw_circle(SkPoint::new(x, y), 1.0, &paint);
            x += step;
        }
        y += step;
    }
}

fn paint_harmony_icon(canvas: &Canvas, width: f32, height: f32, color: SkColor) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.3);
    paint.set_color(color);

    let cx = width * 0.5;
    let cy = height * 0.5;
    let r = 2.6;
    canvas.draw_circle(SkPoint::new(cx, cy - 3.6), r, &paint);
    canvas.draw_circle(SkPoint::new(cx - 3.4, cy + 2.6), r, &paint);
    canvas.draw_circle(SkPoint::new(cx + 3.4, cy + 2.6), r, &paint);
}

fn paint_chevron(canvas: &Canvas, width: f32, height: f32, is_left: bool, color: SkColor) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.6);
    paint.set_stroke_cap(PaintCap::Round);
    paint.set_stroke_join(PaintJoin::Round);
    paint.set_color(color);

    let cx = width * 0.5;
    let cy = height * 0.5;
    let mut pb = PathBuilder::new();
    if is_left {
        pb.move_to(SkPoint::new(cx + 2.5, cy - 4.5));
        pb.line_to(SkPoint::new(cx - 2.5, cy));
        pb.line_to(SkPoint::new(cx + 2.5, cy + 4.5));
    } else {
        pb.move_to(SkPoint::new(cx - 2.5, cy - 4.5));
        pb.line_to(SkPoint::new(cx + 2.5, cy));
        pb.line_to(SkPoint::new(cx - 2.5, cy + 4.5));
    }
    canvas.draw_path(&pb.detach(), &paint);
}

fn paint_preset_swatch(canvas: &Canvas, width: f32, height: f32, preset: GradientPreset) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(PaintStyle::Fill);

    if let Some([c1, c2, c3]) = preset.preview_colors {
        // Fill base with c1, left-top bloom with c2, bottom glow with c3.
        paint.set_color(SkColor::from_rgb(c1.0, c1.1, c1.2));
        canvas.draw_rect(SkRect::from_wh(width, height), &paint);

        paint.set_color(SkColor::from_argb(190, c2.0, c2.1, c2.2));
        canvas.draw_circle(SkPoint::new(width * 0.85, height * 0.25), width * 0.65, &paint);

        paint.set_color(SkColor::from_argb(195, c3.0, c3.1, c3.2));
        canvas.draw_circle(SkPoint::new(width * 0.35, height * 0.92), width * 0.68, &paint);
    } else {
        paint.set_color(SkColor::from_rgb(preset.rgb.0, preset.rgb.1, preset.rgb.2));
        canvas.draw_rect(SkRect::from_wh(width, height), &paint);
    }
}

fn paint_wave_slider(canvas: &Canvas, width: f32, height: f32, intensity: f32, is_dark: bool, show_thumb: bool) {
    let pad_x = 10.0;
    let track_w = (width - pad_x * 2.0).max(10.0);
    let cy = height * 0.5;

    // Rounded pill track behind the sine wave.
    let mut track_paint = Paint::default();
    track_paint.set_anti_alias(true);
    track_paint.set_style(PaintStyle::Fill);
    track_paint.set_color(if is_dark {
        SkColor::from_argb(26, 255, 255, 255)
    } else {
        SkColor::from_argb(26, 0, 0, 0)
    });
    let track_h = 14.0;
    let track_rr = RRect::new_rect_xy(
        SkRect::from_xywh(pad_x, cy - track_h * 0.5, track_w, track_h),
        track_h * 0.5,
        track_h * 0.5,
    );
    canvas.draw_rrect(track_rr, &track_paint);

    // Sine wave path split at `split_x` into active (left) and muted (right) segments.
    let pts = wave_slider_points(intensity, track_w, height, 96);
    let split_x = pad_x + track_w * intensity.clamp(0.0, 1.0);

    let active_color = if !show_thumb {
        if is_dark {
            SkColor::from_argb(128, 161, 161, 161)
        } else {
            SkColor::from_argb(128, 90, 90, 90)
        }
    } else if is_dark {
        SkColor::from_argb(225, 195, 195, 200)
    } else {
        SkColor::from_argb(225, 86, 86, 92)
    };
    let muted_color = if is_dark {
        SkColor::from_argb(95, 161, 161, 161)
    } else {
        SkColor::from_argb(85, 77, 77, 77)
    };

    let mut wave_paint = Paint::default();
    wave_paint.set_anti_alias(true);
    wave_paint.set_style(PaintStyle::Stroke);
    wave_paint.set_stroke_width(3.4);
    wave_paint.set_stroke_cap(PaintCap::Round);
    wave_paint.set_stroke_join(PaintJoin::Round);

    // Draw muted full wave first, then clip and paint active portion on the left of the thumb.
    let mut pb = PathBuilder::new();
    for (idx, (x, y)) in pts.iter().enumerate() {
        let pt = SkPoint::new(pad_x + *x, *y);
        if idx == 0 {
            pb.move_to(pt);
        } else {
            pb.line_to(pt);
        }
    }
    let path = pb.detach();
    wave_paint.set_color(muted_color);
    canvas.draw_path(&path, &wave_paint);

    let save = canvas.save();
    canvas.clip_rect(SkRect::from_xywh(0.0, 0.0, split_x, height), None, false);
    wave_paint.set_color(active_color);
    canvas.draw_path(&path, &wave_paint);
    canvas.restore_to_count(save);

    if show_thumb {
        let thumb_w = 8.0 + intensity.clamp(0.0, 1.0) * 6.0;
        let thumb_h = 30.0 + intensity.clamp(0.0, 1.0) * 10.0;
        let mut thumb_paint = Paint::default();
        thumb_paint.set_anti_alias(true);
        thumb_paint.set_style(PaintStyle::Fill);
        thumb_paint.set_color(if is_dark {
            SkColor::from_rgb(245, 245, 248)
        } else {
            SkColor::from_rgb(24, 24, 28)
        });
        let thumb_rr = RRect::new_rect_xy(
            SkRect::from_xywh(split_x - thumb_w * 0.5, cy - thumb_h * 0.5, thumb_w, thumb_h),
            thumb_w * 0.5,
            thumb_w * 0.5,
        );
        canvas.draw_rrect(thumb_rr, &thumb_paint);
    }
}

fn paint_texture_dial(canvas: &Canvas, width: f32, height: f32, kind: TextureKind, amount: f32, is_dark: bool) {
    let cx = width * 0.5;
    let cy = height * 0.5;
    let outer_r = (width.min(height) * 0.5) - 5.0;
    let face_r = outer_r * 0.64;

    // 16 perimeter detent dots.
    let mut dot_paint = Paint::default();
    dot_paint.set_anti_alias(true);
    dot_paint.set_style(PaintStyle::Fill);

    for i in 0..TEXTURE_DIAL_STEPS {
        let frac = i as f32 / TEXTURE_DIAL_STEPS as f32;
        let angle = frac * 2.0 * PI - PI * 0.5;
        let px = cx + outer_r * cos_f32(angle);
        let py = cy + outer_r * sin_f32(angle);
        let lit = amount > 0.0 && frac <= amount + 0.001;
        dot_paint.set_color(if is_dark {
            SkColor::from_argb(if lit { 210 } else { 68 }, 255, 255, 255)
        } else {
            SkColor::from_argb(if lit { 185 } else { 58 }, 0, 0, 0)
        });
        canvas.draw_circle(SkPoint::new(px, py), 1.8, &dot_paint);
    }

    // Inner circular dial face.
    let mut face_paint = Paint::default();
    face_paint.set_anti_alias(true);
    face_paint.set_style(PaintStyle::Fill);
    face_paint.set_color(if is_dark {
        SkColor::from_argb(38, 255, 255, 255)
    } else {
        SkColor::from_argb(28, 0, 0, 0)
    });
    canvas.draw_circle(SkPoint::new(cx, cy), face_r, &face_paint);

    // Procedural speckled texture inside the dial face (matches the grain preview in gradient-generator-open-light.png).
    if kind != TextureKind::None {
        let save = canvas.save();
        let mut clip_pb = PathBuilder::new();
        clip_pb.add_circle(SkPoint::new(cx, cy), face_r, None);
        canvas.clip_path(&clip_pb.detach(), None, true);

        let grain_alpha = ((if is_dark { 36.0 } else { 52.0 }) * amount.max(0.45)).round() as u8;
        let mut grain_paint = Paint::default();
        grain_paint.set_style(PaintStyle::Fill);
        let step = match kind {
            TextureKind::Grain => 2.2,
            TextureKind::Sand => 3.0,
            TextureKind::Denim => 2.6,
            TextureKind::Tweed => 3.4,
            TextureKind::None => 4.0,
        };
        let mut row = 0u32;
        let mut py = cy - face_r;
        while py <= cy + face_r {
            let mut col = 0u32;
            let mut px = cx - face_r;
            while px <= cx + face_r {
                let hash = (row.wrapping_mul(374_761_393) ^ col.wrapping_mul(668_265_263)).wrapping_mul(1_274_126_177);
                let bright = (hash >> 24) & 1 == 0;
                if !(hash >> 16).is_multiple_of(3) {
                    grain_paint.set_color(if bright {
                        SkColor::from_argb(grain_alpha, 255, 255, 255)
                    } else {
                        SkColor::from_argb(grain_alpha, 0, 0, 0)
                    });
                    canvas.draw_rect(SkRect::from_xywh(px, py, 1.4, 1.4), &grain_paint);
                }
                px += step;
                col += 1;
            }
            py += step;
            row += 1;
        }
        canvas.restore_to_count(save);
    }

    // Dial face 1px ring border.
    let mut ring_paint = Paint::default();
    ring_paint.set_anti_alias(true);
    ring_paint.set_style(PaintStyle::Stroke);
    ring_paint.set_stroke_width(1.0);
    ring_paint.set_color(if is_dark {
        SkColor::from_argb(48, 255, 255, 255)
    } else {
        SkColor::from_argb(44, 0, 0, 0)
    });
    canvas.draw_circle(SkPoint::new(cx, cy), face_r, &ring_paint);

    // Rotary indicator handle bar along the perimeter.
    let angle = amount * 2.0 * PI - PI * 0.5;
    let hx = cx + outer_r * cos_f32(angle);
    let hy = cy + outer_r * sin_f32(angle);
    let save = canvas.save();
    canvas.translate((hx, hy));
    canvas.rotate(angle.to_degrees() + 90.0, None);
    let mut handle_paint = Paint::default();
    handle_paint.set_anti_alias(true);
    handle_paint.set_style(PaintStyle::Fill);
    handle_paint.set_color(if is_dark {
        SkColor::from_rgb(209, 209, 209)
    } else {
        SkColor::from_rgb(117, 117, 117)
    });
    let rr = RRect::new_rect_xy(SkRect::from_xywh(-2.6, -5.5, 5.2, 11.0), 2.0, 2.0);
    canvas.draw_rrect(rr, &handle_paint);
    canvas.restore_to_count(save);
}

fn paint_backdrop_texture(canvas: &Canvas, width: f32, height: f32, kind: TextureKind, amount: f32, is_dark: bool) {
    let spacing = match kind {
        TextureKind::Grain => 7.0,
        TextureKind::Sand => 9.0,
        TextureKind::Denim => 8.0,
        TextureKind::Tweed => 11.0,
        TextureKind::None => return,
    };
    let alpha = ((if is_dark { 22.0 } else { 18.0 }) * amount.clamp(0.0, 1.0)).round() as u8;
    if alpha == 0 {
        return;
    }

    let mut light_paint = Paint::default();
    light_paint.set_style(PaintStyle::Fill);
    light_paint.set_color(SkColor::from_argb(alpha, 255, 255, 255));
    let mut dark_paint = Paint::default();
    dark_paint.set_style(PaintStyle::Fill);
    dark_paint.set_color(SkColor::from_argb(alpha, 0, 0, 0));

    let mut row = 0u32;
    let mut y = 2.0;
    while y < height {
        let mut col = 0u32;
        let mut x = 2.0;
        while x < width {
            let h = (row.wrapping_mul(2_654_435_761) ^ col.wrapping_mul(2_246_822_519)).wrapping_mul(326_648_991);
            if (h >> 28) & 1 == 1 {
                let p = if (h >> 27) & 1 == 0 { &light_paint } else { &dark_paint };
                let jitter_x = ((h >> 16) & 3) as f32;
                let jitter_y = ((h >> 20) & 3) as f32;
                canvas.draw_rect(SkRect::from_xywh(x + jitter_x, y + jitter_y, 1.5, 1.5), p);
            }
            x += spacing;
            col += 1;
        }
        y += spacing;
        row += 1;
    }
}
