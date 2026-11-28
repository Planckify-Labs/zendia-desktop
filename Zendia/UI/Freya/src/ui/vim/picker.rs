/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Zendia Vim picker modal (list and grid layouts), built on the generalized [`PickerList`] and [`PickerGrid`] kit
//! components and driven by `BrowserCore` picker commands and events.

use crate::ui::chrome::sidebar::state::favicon_badge_colors;
use crate::ui::core_view::{CoreView, use_core};
use crate::ui::kit::{PickerGrid, PickerItem as KitPickerItem, PickerList};
use crate::ui::page::PageSurfaceContext;
use crate::ui::theme::{POPUP_OVERLAY_LEVEL, ResolvedScheme, ResolvedTheme, Rgba, ThemeRole, motion, use_theme};
use crate::ui::vim::statusline::try_use_vim_statusline_state;
use freya::animation::*;
use freya::prelude::*;
use zendia_browser_core::commands::Command;
use zendia_browser_core::picker::{
    DEFAULT_GRID_COLUMNS, PickerItem as CorePickerItem, PickerLayout, PickerRequest, filter_picker_items,
    find_item_by_key_badge, step_picker_cursor,
};

/// View-local state for the window's active Zendia Vim picker modal.
#[derive(Clone, Copy, PartialEq)]
pub struct VimPickerViewState {
    pub request: State<Option<PickerRequest>>,
    pub query: State<String>,
    pub cursor: State<usize>,
    pub filter_active: State<bool>,
    pub open_seq: State<u64>,
    pub focus_a11y_id: AccessibilityId,
}

impl VimPickerViewState {
    /// Opens a picker modal from a core-published [`PickerRequest`].
    pub fn open(&self, request: PickerRequest) {
        let initial = request.initial_selected;
        let mut req_state = self.request;
        let mut query = self.query;
        let mut cursor = self.cursor;
        let mut filter_active = self.filter_active;
        let mut seq = self.open_seq;

        query.set(String::new());
        filter_active.set(false);
        cursor.set(initial);
        req_state.set(Some(request));
        let next_seq = seq.peek().wrapping_add(1);
        seq.set(next_seq);
        self.focus_a11y_id.request_focus();
    }

    /// Closes the picker modal and resets transient query/cursor state.
    pub fn close(&self) {
        let mut req_state = self.request;
        let mut query = self.query;
        let mut filter_active = self.filter_active;
        req_state.set_if_modified(None);
        query.set_if_modified(String::new());
        filter_active.set_if_modified(false);
        self.focus_a11y_id.request_unfocus();
    }

    /// Returns whether a Vim picker modal is currently open.
    pub fn is_open(&self) -> bool {
        self.request.peek().is_some()
    }
}

/// Installs [`VimPickerViewState`] into Freya context for the current window.
pub fn use_provide_vim_picker_state() -> VimPickerViewState {
    let request = use_state(|| None::<PickerRequest>);
    let query = use_state(String::new);
    let cursor = use_state(|| 0usize);
    let filter_active = use_state(|| false);
    let open_seq = use_state(|| 0u64);
    let focus_a11y_id = use_hook(AccessibilityId::new_unique);

    use_provide_context(|| VimPickerViewState {
        request,
        query,
        cursor,
        filter_active,
        open_seq,
        focus_a11y_id,
    })
}

/// Consumes the window's [`VimPickerViewState`] from Freya context.
pub fn use_vim_picker_state() -> VimPickerViewState {
    use_consume::<VimPickerViewState>()
}

/// Consumes [`VimPickerViewState`] if a provider is installed in this window.
pub fn try_use_vim_picker_state() -> Option<VimPickerViewState> {
    try_consume_context::<VimPickerViewState>()
}

/// Authentic vector brand emblems for built-in AI agents so every tile and badge renders sharply at 1x and 2x.
pub fn agent_emblem_svg(icon_id: &str, dimmed: bool) -> Option<(String, Bytes)> {
    let suffix = if dimmed { "dim" } else { "full" };
    let svg = match icon_id {
        "agent-claude" => {
            let color = if dimmed { "#6e7280" } else { "#D97757" };
            format!(
                r##"<svg height="24" viewBox="0 0 24 24" width="24" xmlns="http://www.w3.org/2000/svg"><path d="M4.709 15.955l4.72-2.647.08-.23-.08-.128H9.2l-.79-.048-2.698-.073-2.339-.097-2.266-.122-.571-.121L0 11.784l.055-.352.48-.321.686.06 1.52.103 2.278.158 1.652.097 2.449.255h.389l.055-.157-.134-.098-.103-.097-2.358-1.596-2.552-1.688-1.336-.972-.724-.491-.364-.462-.158-1.008.656-.722.881.06.225.061.893.686 1.908 1.476 2.491 1.833.365.304.145-.103.019-.073-.164-.274-1.355-2.446-1.446-2.49-.644-1.032-.17-.619a2.97 2.97 0 01-.104-.729L6.283.134 6.696 0l.996.134.42.364.62 1.414 1.002 2.229 1.555 3.03.456.898.243.832.091.255h.158V9.01l.128-1.706.237-2.095.23-2.695.08-.76.376-.91.747-.492.584.28.48.685-.067.444-.286 1.851-.559 2.903-.364 1.942h.212l.243-.242.985-1.306 1.652-2.064.73-.82.85-.904.547-.431h1.033l.76 1.129-.34 1.166-1.064 1.347-.881 1.142-1.264 1.7-.79 1.36.073.11.188-.02 2.856-.606 1.543-.28 1.841-.315.833.388.091.395-.328.807-1.969.486-2.309.462-3.439.813-.042.03.049.061 1.549.146.662.036h1.622l3.02.225.79.522.474.638-.079.485-1.215.62-1.64-.389-3.829-.91-1.312-.329h-.182v.11l1.093 1.068 2.006 1.81 2.509 2.33.127.578-.322.455-.34-.049-2.205-1.657-.851-.747-1.926-1.62h-.128v.17l.444.649 2.345 3.521.122 1.08-.17.353-.608.213-.668-.122-1.374-1.925-1.415-2.167-1.143-1.943-.14.08-.674 7.254-.316.37-.729.28-.607-.461-.322-.747.322-1.476.389-1.924.315-1.53.286-1.9.17-.632-.012-.042-.14.018-1.434 1.967-2.18 2.945-1.726 1.845-.414.164-.717-.37.067-.662.401-.589 2.388-3.036 1.44-1.882.93-1.086-.006-.158h-.055L4.132 18.56l-1.13.146-.487-.456.061-.746.231-.243 1.908-1.312-.006.006z" fill="{color}" fill-rule="nonzero"/></svg>"##
            )
        }
        "agent-codex" => {
            if dimmed {
                r##"<svg height="24" viewBox="0 0 24 24" width="24" xmlns="http://www.w3.org/2000/svg"><path d="M9.064 3.344a4.578 4.578 0 012.285-.312c1 .115 1.891.54 2.673 1.275.01.01.024.017.037.021a.09.09 0 00.043 0 4.55 4.55 0 013.046.275l.047.022.116.057a4.581 4.581 0 012.188 2.399c.209.51.313 1.041.315 1.595a4.24 4.24 0 01-.134 1.223.123.123 0 00.03.115c.594.607.988 1.33 1.183 2.17.289 1.425-.007 2.71-.887 3.854l-.136.166a4.548 4.548 0 01-2.201 1.388.123.123 0 00-.081.076c-.191.551-.383 1.023-.74 1.494-.9 1.187-2.222 1.846-3.711 1.838-1.187-.006-2.239-.44-3.157-1.302a.107.107 0 00-.105-.024c-.388.125-.78.143-1.204.138a4.441 4.441 0 01-1.945-.466 4.544 4.544 0 01-1.61-1.335c-.152-.202-.303-.392-.414-.617a5.81 5.81 0 01-.37-.961 4.582 4.582 0 01-.014-2.298.124.124 0 00.006-.056.085.085 0 00-.027-.048 4.467 4.467 0 01-1.034-1.651 3.896 3.896 0 01-.251-1.192 5.189 5.189 0 01.141-1.6c.337-1.112.982-1.985 1.933-2.618.212-.141.413-.251.601-.33.215-.089.43-.164.646-.227a.098.098 0 00.065-.066 4.51 4.51 0 01.829-1.615 4.535 4.535 0 011.837-1.388zm3.482 10.565a.637.637 0 000 1.272h3.636a.637.637 0 100-1.272h-3.636zM8.462 9.23a.637.637 0 00-1.106.631l1.272 2.224-1.266 2.136a.636.636 0 101.095.649l1.454-2.455a.636.636 0 00.005-.64L8.462 9.23z" fill="#6e7280"/></svg>"##
                    .to_string()
            } else {
                r##"<svg height="24" viewBox="0 0 24 24" width="24" xmlns="http://www.w3.org/2000/svg"><path d="M9.064 3.344a4.578 4.578 0 012.285-.312c1 .115 1.891.54 2.673 1.275.01.01.024.017.037.021a.09.09 0 00.043 0 4.55 4.55 0 013.046.275l.047.022.116.057a4.581 4.581 0 012.188 2.399c.209.51.313 1.041.315 1.595a4.24 4.24 0 01-.134 1.223.123.123 0 00.03.115c.594.607.988 1.33 1.183 2.17.289 1.425-.007 2.71-.887 3.854l-.136.166a4.548 4.548 0 01-2.201 1.388.123.123 0 00-.081.076c-.191.551-.383 1.023-.74 1.494-.9 1.187-2.222 1.846-3.711 1.838-1.187-.006-2.239-.44-3.157-1.302a.107.107 0 00-.105-.024c-.388.125-.78.143-1.204.138a4.441 4.441 0 01-1.945-.466 4.544 4.544 0 01-1.61-1.335c-.152-.202-.303-.392-.414-.617a5.81 5.81 0 01-.37-.961 4.582 4.582 0 01-.014-2.298.124.124 0 00.006-.056.085.085 0 00-.027-.048 4.467 4.467 0 01-1.034-1.651 3.896 3.896 0 01-.251-1.192 5.189 5.189 0 01.141-1.6c.337-1.112.982-1.985 1.933-2.618.212-.141.413-.251.601-.33.215-.089.43-.164.646-.227a.098.098 0 00.065-.066 4.51 4.51 0 01.829-1.615 4.535 4.535 0 011.837-1.388zm3.482 10.565a.637.637 0 000 1.272h3.636a.637.637 0 100-1.272h-3.636zM8.462 9.23a.637.637 0 00-1.106.631l1.272 2.224-1.266 2.136a.636.636 0 101.095.649l1.454-2.455a.636.636 0 00.005-.64L8.462 9.23z" fill="url(#codex-gradient)"/><defs><linearGradient gradientUnits="userSpaceOnUse" id="codex-gradient" x1="12" x2="12" y1="3" y2="21"><stop stop-color="#B1A7FF"/><stop offset=".5" stop-color="#7A9DFF"/><stop offset="1" stop-color="#3941FF"/></linearGradient></defs></svg>"##
                    .to_string()
            }
        }
        "agent-cursor" => {
            let color = if dimmed { "#6e7280" } else { "#1f2430" };
            format!(
                r##"<svg fill-rule="evenodd" height="24" viewBox="0 0 24 24" width="24" xmlns="http://www.w3.org/2000/svg"><path fill="{color}" d="M22.106 5.68L12.5.135a.998.998 0 00-.998 0L1.893 5.68a.84.84 0 00-.419.726v11.186c0 .3.16.577.42.727l9.607 5.547a.999.999 0 00.998 0l9.608-5.547a.84.84 0 00.42-.727V6.407a.84.84 0 00-.42-.726zm-.603 1.176L12.228 22.92c-.063.108-.228.064-.228-.061V12.34a.59.59 0 00-.295-.51l-9.11-5.26c-.107-.062-.063-.228.062-.228h18.55c.264 0 .428.286.296.514z"/></svg>"##
            )
        }
        "agent-antigravity" => {
            if dimmed {
                r##"<svg height="24" viewBox="0 0 24 24" width="24" xmlns="http://www.w3.org/2000/svg"><path d="M21.751 22.607c1.34 1.005 3.35.335 1.508-1.508C17.73 15.74 18.904 1 12.037 1 5.17 1 6.342 15.74.815 21.1c-2.01 2.009.167 2.511 1.507 1.506 5.192-3.517 4.857-9.714 9.715-9.714 4.857 0 4.522 6.197 9.714 9.715z" fill="#6e7280"/></svg>"##
                    .to_string()
            } else {
                r##"<svg height="24" viewBox="0 0 24 24" width="24" xmlns="http://www.w3.org/2000/svg"><defs><linearGradient id="ag-real" x1="2" y1="22" x2="22" y2="2" gradientUnits="userSpaceOnUse"><stop offset="0%" stop-color="#00B95C"/><stop offset="35%" stop-color="#3186FF"/><stop offset="72%" stop-color="#FC413D"/><stop offset="100%" stop-color="#FBBC04"/></linearGradient></defs><path d="M21.751 22.607c1.34 1.005 3.35.335 1.508-1.508C17.73 15.74 18.904 1 12.037 1 5.17 1 6.342 15.74.815 21.1c-2.01 2.009.167 2.511 1.507 1.506 5.192-3.517 4.857-9.714 9.715-9.714 4.857 0 4.522 6.197 9.714 9.715z" fill="url(#ag-real)"/></svg>"##
                    .to_string()
            }
        }
        "agent-opencode" => {
            let color = if dimmed { "#6e7280" } else { "#1f2430" };
            format!(
                r##"<svg width="300" height="300" viewBox="0 0 300 300" fill="none" xmlns="http://www.w3.org/2000/svg"><g transform="translate(30, 0)"><path d="M180 240H60V120H180V240Z" fill="{color}" fill-opacity="0.4"/><path d="M180 60H60V240H180V60ZM240 300H0V0H240V300Z" fill="{color}"/></g></svg>"##
            )
        }
        "agent-kiro" => {
            let bg = if dimmed { "#6e7280" } else { "#9046FF" };
            format!(
                r##"<svg height="24" viewBox="0 0 24 24" width="24" xmlns="http://www.w3.org/2000/svg"><path d="M18.8 0H5.2A5.2 5.2 0 000 5.2v13.6A5.2 5.2 0 005.2 24h13.6a5.2 5.2 0 005.2-5.2V5.2A5.2 5.2 0 0018.8 0z" fill="{bg}"/><path d="M7.97 16.376c-1.644 3.642 1.86 4.556 4.443 2.424.76 2.39 3.608.607 4.631-1.247 2.251-4.084 1.342-8.249 1.108-9.108-1.6-5.859-9.6-5.869-10.976.03-.323 1.033-.328 2.206-.507 3.423-.09.617-.16 1.009-.393 1.655-.139.373-.323.7-.62 1.257-.458.865-.264 2.53 2.101 1.665l.224-.1h-.01l-.001.001z" fill="#fff"/><path d="M12.722 10.985c-.656 0-.755-.785-.755-1.252 0-.423.074-.756.218-.97a.61.61 0 01.537-.283c.229 0 .428.095.567.289.159.218.243.55.243.964 0 .785-.303 1.252-.805 1.252h-.005zm2.703 0c-.656 0-.755-.785-.755-1.252 0-.423.074-.756.219-.97a.61.61 0 01.536-.283c.229 0 .428.095.567.289.159.218.243.55.243.964 0 .785-.303 1.252-.805 1.252h-.005z" fill="#000"/></svg>"##
            )
        }
        _ => return None,
    };
    Some((
        format!("zendia-agent-emblem-{icon_id}-{suffix}-v2"),
        Bytes::from(svg.into_bytes()),
    ))
}

fn core_item_to_kit_item(item: &CorePickerItem, layout: PickerLayout, theme: &ResolvedTheme) -> KitPickerItem {
    let mut kit = KitPickerItem::new(item.id.clone(), item.title.clone())
        .dimmed(item.dimmed)
        .disabled(!item.enabled)
        .icon_fill(item.icon_fill);

    if let Some(section) = &item.section {
        kit = kit.section(section.clone());
    }
    if let Some(detail) = &item.detail {
        kit = kit.detail(detail.clone());
    }
    if let Some(key) = &item.key {
        kit = kit.key_badge(key.clone());
    }
    if let Some((r, g, b)) = item.accent_rgb {
        kit = kit.accent_color(Color::from_rgb(r, g, b));
    }
    if let Some(tooltip) = &item.tooltip {
        kit = kit.tooltip(tooltip.clone());
    }
    if let Some(icon_data) = &item.icon_data {
        kit = kit.extension_icon(icon_data.clone());
    }

    if let Some(icon) = &item.icon {
        if let Some((cache_key, bytes)) = agent_emblem_svg(icon, item.dimmed) {
            kit = kit.custom_icon_svg(cache_key, bytes);
        } else {
            kit = kit.icon(icon.clone());
            if layout == PickerLayout::List {
                let (bg, fg) = if let Some((r, g, b)) = item.icon_badge_rgb {
                    (Color::from_rgb(r, g, b), Color::WHITE)
                } else {
                    favicon_badge_colors(Some(icon.as_str()), theme)
                };
                kit = kit.icon_badge(bg, fg);
            }
        }
    }

    kit
}

fn invoke_choose(core: &CoreView, request: &PickerRequest, item: &CorePickerItem) {
    if !item.enabled {
        return;
    }
    core.invoke_or_log(Command::PickerChoose {
        request_id: Some(request.id.clone()),
        item_id: item.id.clone(),
    });
}

/// Computes the modal's `(width, top)` inside a window or stage of `(viewport_w, viewport_h)`.
pub fn compute_picker_geometry(layout: PickerLayout, viewport_w: f32, viewport_h: f32) -> (f32, f32) {
    let max_w = match layout {
        PickerLayout::List => 560.0,
        PickerLayout::Grid => 500.0,
    };
    let width = (viewport_w * 0.80).min(max_w).max(280.0);
    let top = (viewport_h * 0.20).clamp(32.0, (viewport_h - 180.0).max(32.0));
    (width, top)
}

/// Window-root overlay rendering the active Zendia Vim picker modal above all chrome and pages.
///
/// Only shows when `model.vim_mode_enabled` is true.
#[derive(Clone, PartialEq)]
pub struct VimPickerOverlay;

impl Component for VimPickerOverlay {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let state = use_vim_picker_state();
        let page_ctx = try_consume_context::<PageSurfaceContext>();
        let statusline_state = try_use_vim_statusline_state();

        let vim_enabled = core.model().read().vim_mode_enabled;
        let current_request = state.request.read().clone();
        let is_open = vim_enabled && current_request.is_some();
        let open_seq = *state.open_seq.read();

        let mut last_request = use_state(|| None::<PickerRequest>);
        if let Some(req) = &current_request
            && last_request.peek().as_ref() != Some(req)
        {
            last_request.set(Some(req.clone()));
        }

        use_drop(move || {
            if let Some(sl) = statusline_state {
                sl.set_overlay_obstacle(None);
            }
        });
        if !is_open && let Some(sl) = statusline_state {
            sl.set_overlay_obstacle(None);
        }

        let entrance = use_animation_with_dependencies(&(is_open, open_seq), move |conf, (open, _)| {
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

        let progress = entrance.get().value().clamp(0.0, 1.05);
        let opacity = progress.clamp(0.0, 1.0);
        if !is_open && opacity <= 0.01 {
            return rect().into_element();
        }

        let Some(request) = current_request.or_else(|| last_request.read().clone()) else {
            return rect().into_element();
        };

        let root_size = *Platform::get().current_window().root_size.read();
        let window_w = root_size.width.max(480.0);
        let window_h = root_size.height.max(360.0);
        let (card_w, base_top) = compute_picker_geometry(request.layout, window_w, window_h);
        let max_list_h = (window_h * 0.60 - 82.0).clamp(160.0, 520.0);
        let left = ((window_w - card_w) * 0.5).max(16.0);
        let (y_offset, scale) = if is_open {
            (
                motion::FLOAT_IN_Y * (1.0 - progress),
                motion::FLOAT_IN_SCALE + (1.0 - motion::FLOAT_IN_SCALE) * progress,
            )
        } else {
            let exit_t = (1.0 - opacity).clamp(0.0, 1.0);
            (
                motion::FLOAT_OUT_Y * exit_t,
                1.0 + (motion::FLOAT_OUT_SCALE - 1.0) * exit_t,
            )
        };
        let top = (base_top + y_offset).max(20.0);

        let close_picker = {
            let core = core.clone();
            let page_ctx = page_ctx.clone();
            let request_id = request.id.clone();
            move || {
                state.close();
                core.invoke_or_log(Command::PickerClose {
                    request_id: Some(request_id.clone()),
                });
                if let Some(ctx) = &page_ctx {
                    ctx.request_focus_active_surface();
                }
            }
        };
        let close_backdrop = close_picker.clone();
        let close_card = close_picker;

        rect()
            .a11y_id(state.focus_a11y_id)
            .a11y_focusable(true)
            .layer(Layer::OverlayLevel(POPUP_OVERLAY_LEVEL))
            .position(Position::new_global().left(0.0).top(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .on_mouse_down(move |_| {
                close_backdrop();
            })
            .child(
                rect()
                    .position(Position::new_global().left(left).top(top))
                    .opacity(opacity)
                    .scale((scale, scale))
                    .on_sized(move |e: Event<SizedEventData>| {
                        if is_open && let Some(sl) = statusline_state {
                            sl.set_overlay_obstacle(Some((
                                e.area.min_x(),
                                e.area.min_y(),
                                e.area.width(),
                                e.area.height(),
                            )));
                        }
                    })
                    .child(
                        VimPickerCard::new(request, state.query, state.cursor, state.filter_active)
                            .width(card_w)
                            .max_list_height(max_list_h)
                            .stagger_trigger(open_seq)
                            .capture_keys(is_open)
                            .on_close(move |_| close_card())
                            .key(open_seq),
                    ),
            )
            .into_element()
    }
}

/// Reusable Zendia Vim picker modal card used by [`VimPickerOverlay`] and by the `vim-pickers` gallery section.
#[derive(Clone, PartialEq)]
pub struct VimPickerCard {
    request: PickerRequest,
    query: State<String>,
    cursor: State<usize>,
    filter_active: State<bool>,
    width: Option<f32>,
    max_list_height: f32,
    stagger_trigger: u64,
    capture_keys: bool,
    on_choose: Option<EventHandler<CorePickerItem>>,
    on_close: Option<EventHandler<()>>,
    key: DiffKey,
}

impl KeyExt for VimPickerCard {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl VimPickerCard {
    pub fn new(request: PickerRequest, query: State<String>, cursor: State<usize>, filter_active: State<bool>) -> Self {
        Self {
            request,
            query,
            cursor,
            filter_active,
            width: None,
            max_list_height: 360.0,
            stagger_trigger: 0,
            capture_keys: false,
            on_choose: None,
            on_close: None,
            key: DiffKey::None,
        }
    }

    pub fn stagger_trigger(mut self, trigger: u64) -> Self {
        self.stagger_trigger = trigger;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn max_list_height(mut self, height: f32) -> Self {
        self.max_list_height = height;
        self
    }

    pub fn capture_keys(mut self, capture: bool) -> Self {
        self.capture_keys = capture;
        self
    }

    pub fn on_choose(mut self, handler: impl Into<EventHandler<CorePickerItem>>) -> Self {
        self.on_choose = Some(handler.into());
        self
    }

    pub fn on_close(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_close = Some(handler.into());
        self
    }
}

impl Component for VimPickerCard {
    fn render(&self) -> impl IntoElement {
        let core = use_core();
        let theme = use_theme();

        let query_text = self.query.read().clone();
        let filtered = filter_picker_items(&self.request, &query_text);
        let item_count = filtered.len();

        let mut cursor = self.cursor;
        let current_cursor = if item_count == 0 {
            None
        } else {
            let clamped = (*cursor.read()).min(item_count - 1);
            if *cursor.peek() != clamped {
                cursor.set(clamped);
            }
            Some(clamped)
        };

        let kit_items: Vec<KitPickerItem> = filtered
            .iter()
            .map(|item| core_item_to_kit_item(item, self.request.layout, &theme))
            .collect();

        let choose_item = {
            let core = core.clone();
            let request = self.request.clone();
            let on_choose = self.on_choose.clone();
            let on_close = self.on_close.clone();
            move |item: &CorePickerItem| {
                if !item.enabled {
                    return;
                }
                if let Some(handler) = &on_choose {
                    handler.call(item.clone());
                } else {
                    invoke_choose(&core, &request, item);
                    if let Some(close) = &on_close {
                        close.call(());
                    }
                }
            }
        };

        let choose_on_click = {
            let filtered = filtered.clone();
            let choose_item = choose_item.clone();
            move |idx: usize| {
                if let Some(item) = filtered.get(idx) {
                    choose_item(item);
                }
            }
        };

        let handle_global_key = {
            let filtered = filtered.clone();
            let layout = self.request.layout;
            let capture = self.capture_keys;
            let choose_item = choose_item;
            let on_close = self.on_close.clone();
            move |e: Event<KeyboardEventData>| {
                if !capture {
                    return;
                }
                // Swallow every key while the picker is open (`ZendiaPicker.sys.mjs:196-197`, `PIC-01`, `PIC-02`).
                e.stop_propagation();
                e.prevent_default();

                let ctrl = e.modifiers.contains(Modifiers::CONTROL);
                let alt = e.modifiers.contains(Modifiers::ALT);
                let meta = e.modifiers.contains(Modifiers::META);

                let mut move_cursor = |delta: isize| {
                    let current = *cursor.peek();
                    if let Some(next) =
                        step_picker_cursor(current, filtered.len(), layout, DEFAULT_GRID_COLUMNS, 0, delta)
                    {
                        cursor.set(next);
                    }
                };

                match &e.key {
                    Key::Named(NamedKey::Escape) => {
                        if let Some(close) = &on_close {
                            close.call(());
                        }
                    }
                    Key::Character(ch) if ctrl && !alt && !meta => {
                        let lower = ch.to_ascii_lowercase();
                        if lower == "c" || lower == "[" || e.code == Code::KeyC || e.code == Code::BracketLeft {
                            if let Some(close) = &on_close {
                                close.call(());
                            }
                        } else if lower == "n" || e.code == Code::KeyN {
                            move_cursor(1);
                        } else if lower == "p" || e.code == Code::KeyP {
                            move_cursor(-1);
                        }
                    }
                    Key::Named(NamedKey::Enter) => {
                        let idx = (*cursor.peek()).min(filtered.len().saturating_sub(1));
                        if let Some(item) = filtered.get(idx) {
                            choose_item(item);
                        }
                    }
                    Key::Named(NamedKey::ArrowDown | NamedKey::ArrowRight) => {
                        move_cursor(1);
                    }
                    Key::Named(NamedKey::ArrowUp | NamedKey::ArrowLeft) => {
                        move_cursor(-1);
                    }
                    Key::Character(ch) if !ctrl && !alt && !meta => {
                        if ch == "j" {
                            move_cursor(1);
                        } else if ch == "k" {
                            move_cursor(-1);
                        } else if let Some(jump_idx) = find_item_by_key_badge(&filtered, ch) {
                            cursor.set(jump_idx);
                            if let Some(item) = filtered.get(jump_idx) {
                                choose_item(item);
                            }
                        }
                    }
                    _ => {}
                }
            }
        };

        let card_width = self.width.unwrap_or(match self.request.layout {
            PickerLayout::List => 560.0,
            PickerLayout::Grid => 500.0,
        });

        let glass_bg = if theme.glass.float_blur > 0.0 {
            theme.glass.float_tint.to_color()
        } else {
            theme.glass.float_solid.to_color()
        };
        let outline_color = theme.glass.outline.to_color();
        let text_muted = theme
            .rgba(ThemeRole::Text)
            .with_alpha(theme.glass.muted_opacity)
            .to_color();
        let divider_color = theme.glass.outline.to_color();
        let sheen_gradient = LinearGradient::new()
            .angle(0.0)
            .stop((theme.glass.sheen_top.to_color(), 0.0))
            .stop((Color::from_argb(0, 255, 255, 255), 100.0));

        // Hover applies only a subtle row/tile hover tint inside `PickerList`/`PickerGrid` without moving the
        // keyboard selection cursor (`VIM-PK-06`).
        let body: Element = match self.request.layout {
            PickerLayout::List => PickerList::new(kit_items)
                .vim_style(true)
                .highlighted(current_cursor)
                .max_height(self.max_list_height)
                .stagger_trigger(self.stagger_trigger)
                .empty_message("No matching items")
                .on_select(choose_on_click)
                .into_element(),
            PickerLayout::Grid => PickerGrid::new(kit_items)
                .columns(DEFAULT_GRID_COLUMNS)
                .highlighted(current_cursor)
                .max_height(self.max_list_height)
                .stagger_trigger(self.stagger_trigger)
                .empty_message("No matching items")
                .on_select(choose_on_click)
                .into_element(),
        };

        let hint_text = self.request.hint.clone();

        let inner_ring_color = match theme.scheme {
            ResolvedScheme::Dark => Rgba::from_rgb_alpha(255, 255, 255, 0.05).to_color(),
            ResolvedScheme::Light => Rgba::from_rgb_alpha(255, 255, 255, 0.30).to_color(),
        };

        let card = rect()
            .a11y_role(AccessibilityRole::Dialog)
            .a11y_alt(self.request.title.clone())
            .width(Size::px(card_width))
            .corner_radius(CornerRadius::new_all(theme.radii.float))
            .overflow(Overflow::Clip)
            .background(glass_bg)
            .border(
                Border::new()
                    .width(0.5)
                    .alignment(BorderAlignment::Outer)
                    .fill(outline_color),
            )
            .shadow(theme.shadows.float_highlight.to_shadow())
            .shadow(theme.shadows.float_ambient.to_shadow())
            .shadow(theme.shadows.float_drop.to_shadow())
            .on_mouse_down(|e: Event<MouseEventData>| e.stop_propagation())
            .on_global_key_down(handle_global_key)
            .child(
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .width(Size::percent(100.0))
                    .height(Size::px(48.0))
                    .background(sheen_gradient)
                    .interactive(Interactive::No),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .padding(Gaps::new_all(7.0))
                    .corner_radius(CornerRadius::new_all(theme.radii.float))
                    .border(
                        Border::new()
                            .width(0.5)
                            .alignment(BorderAlignment::Inner)
                            .fill(inner_ring_color),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new(6.0, 10.0, 8.0, 10.0))
                            .child(
                                label()
                                    .text(self.request.title.clone())
                                    .font_size(13.0)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(text_muted)
                                    .max_lines(1)
                                    .text_overflow(TextOverflow::Ellipsis),
                            ),
                    )
                    .child(body)
                    .child(
                        rect()
                            .width(Size::fill())
                            .margin(Gaps::new(6.0, 0.0, 0.0, 0.0))
                            .child(
                                rect()
                                    .width(Size::fill())
                                    .height(Size::px(0.5))
                                    .background(divider_color),
                            )
                            .child(
                                rect()
                                    .width(Size::fill())
                                    .padding(Gaps::new(8.0, 10.0, 2.0, 10.0))
                                    .child(
                                        label()
                                            .text(hint_text)
                                            .font_size(11.0)
                                            .color(text_muted)
                                            .max_lines(1)
                                            .text_overflow(TextOverflow::Ellipsis),
                                    ),
                            ),
                    ),
            );

        if theme.glass.float_blur > 0.0 {
            card.backdrop_blur(theme.glass.float_blur)
        } else {
            card
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_geometry_respects_list_and_grid_widths() {
        let (list_w, list_top) = compute_picker_geometry(PickerLayout::List, 1600.0, 1000.0);
        assert_eq!(list_w, 560.0);
        assert_eq!(list_top, 200.0);

        let (grid_w, grid_top) = compute_picker_geometry(PickerLayout::Grid, 1600.0, 1000.0);
        assert_eq!(grid_w, 500.0);
        assert_eq!(grid_top, 200.0);

        let (narrow_w, _) = compute_picker_geometry(PickerLayout::List, 500.0, 600.0);
        assert_eq!(narrow_w, 400.0);
    }

    #[test]
    fn agent_emblems_parse_in_skia_for_active_and_dimmed_variants() {
        use freya::engine::prelude::{FontMgr, svg};

        for id in [
            "agent-claude",
            "agent-codex",
            "agent-cursor",
            "agent-antigravity",
            "agent-opencode",
            "agent-kiro",
        ] {
            for dimmed in [false, true] {
                let (_, bytes) = agent_emblem_svg(id, dimmed).expect("emblem should exist");
                let dom = svg::Dom::from_bytes(&bytes, FontMgr::empty());
                assert!(
                    dom.is_ok(),
                    "Skia failed to parse {id} (dimmed={dimmed}): {:?}",
                    dom.err()
                );
            }
        }
    }
}
