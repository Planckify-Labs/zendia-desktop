/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Side panel dock preferences, agent session data, model/effort ranking, tables, instructions, and motion math.
//!
//! Pure domain types and algorithms live here so Freya, headless runners, and unit tests all share one source of truth.

use crate::contributions::{SidePanelLayout, SidePanelSide};
use crate::model::{ModelEffort, ModelId, ModelInfo, TabId};
use std::cmp::Ordering;
use std::f32::consts::PI;
use zendia_module_api::ui::UiNode;

/// Built-in side panel identifier for Zendia's AI agent panel.
pub const AGENT_PANEL_ID: &str = "zendia.agent";
/// Built-in side panel identifier for Zendia's cross-window agents roster panel.
pub const AGENTS_PANEL_ID: &str = "zendia.agents";
/// Default width of the agents roster side panel (`340 px`, matching `ZendiaAgentsPanel.sys.mjs`).
pub const AGENTS_PANEL_DEFAULT_WIDTH: f32 = 340.0;
/// Minimum logical width of the agents roster side panel (`280 px`, matching `ZendiaAgentsPanel.sys.mjs`).
pub const AGENTS_PANEL_MIN_WIDTH: f32 = 280.0;
/// Default and minimum logical width of the agent side panel (`420 px`).
pub const PANEL_MIN_WIDTH: f32 = 420.0;
/// Maximum logical width of a side panel (`820 px`).
pub const PANEL_MAX_WIDTH: f32 = 820.0;
/// Default width of the agent side panel (`420 px`).
pub const PANEL_DEFAULT_WIDTH: f32 = 420.0;
/// Default width of the web chat side panel (`480 px`, matching `ZendiaWebChatDock.sys.mjs`).
pub const WEBCHAT_PANEL_DEFAULT_WIDTH: f32 = 480.0;
/// Minimum usable width a page (or narrowest split pane) must keep beside a split-docked side panel.
pub const MIN_PAGE_PANE_WIDTH: f32 = 420.0;
/// Inset around a floating side panel card (`10 px`).
pub const PANEL_FLOAT_INSET: f32 = 10.0;
/// Gap between a split-docked side panel and the page card (`8 px`).
pub const PANEL_SPLIT_GAP: f32 = 8.0;

/// Profile settings controlling how one side panel docks, resizes, and falls back when narrow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SidePanelPrefs {
    pub layout: SidePanelLayout,
    pub side: SidePanelSide,
    pub width: f32,
    /// Layout used temporarily when `layout == Split` has too little room beside the page.
    pub split_fallback: SidePanelLayout,
}

impl Default for SidePanelPrefs {
    fn default() -> Self {
        Self {
            layout: SidePanelLayout::Float,
            side: SidePanelSide::Right,
            width: PANEL_DEFAULT_WIDTH,
            split_fallback: SidePanelLayout::Float,
        }
    }
}

impl SidePanelPrefs {
    pub fn min_width_for(panel_id: &str) -> f32 {
        if panel_id == AGENTS_PANEL_ID {
            AGENTS_PANEL_MIN_WIDTH
        } else {
            PANEL_MIN_WIDTH
        }
    }

    pub fn clamp_width(width: f32) -> f32 {
        if width.is_finite() {
            width.max(PANEL_MIN_WIDTH)
        } else {
            PANEL_DEFAULT_WIDTH
        }
    }

    pub fn default_width_for(panel_id: &str) -> f32 {
        crate::shell_layout::builtin_panel_default_width(panel_id)
    }

    pub fn default_for(panel_id: &str) -> Self {
        Self {
            width: Self::default_width_for(panel_id),
            side: if panel_id == AGENTS_PANEL_ID {
                SidePanelSide::Left
            } else {
                SidePanelSide::Right
            },
            ..Self::default()
        }
    }

    pub fn clamp_width_for(panel_id: &str, width: f32) -> f32 {
        let min_w = Self::min_width_for(panel_id);
        if width.is_finite() {
            width.max(min_w)
        } else {
            Self::default_width_for(panel_id)
        }
    }
}

/// Computes the widest width a split-docked side panel can occupy while keeping every page pane at least
/// [`MIN_PAGE_PANE_WIDTH`] wide.
///
/// `container_width` is the total width available for the page area plus the split panel; `min_pane_share` is the
/// narrowest split-view pane's fraction of the page area (`1.0` for a single tab).
pub fn split_max_width(container_width: f32, min_pane_share: f32) -> f32 {
    if !container_width.is_finite() || container_width <= 0.0 {
        return f32::INFINITY;
    }
    let share = min_pane_share.clamp(0.1, 1.0);
    (container_width - PANEL_SPLIT_GAP - MIN_PAGE_PANE_WIDTH / share).floor()
}

/// Resolved layout geometry for a side panel given its preferences and the current stage width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedPanelLayout {
    pub effective_layout: SidePanelLayout,
    pub width: f32,
    pub max_width: f32,
    pub fell_back: bool,
    pub resize_disabled: bool,
}

/// Resolves the effective layout and clamped width of a side panel inside `stage_width`.
pub fn resolve_panel_layout(prefs: SidePanelPrefs, stage_width: f32, min_pane_share: f32) -> ResolvedPanelLayout {
    resolve_panel_layout_with_min(prefs, stage_width, min_pane_share, PANEL_MIN_WIDTH)
}

/// Resolves the effective layout and clamped width of a side panel with an explicit minimum width.
pub fn resolve_panel_layout_with_min(
    prefs: SidePanelPrefs,
    stage_width: f32,
    min_pane_share: f32,
    min_width: f32,
) -> ResolvedPanelLayout {
    let split_max = if prefs.layout == SidePanelLayout::Split {
        split_max_width(stage_width, min_pane_share)
    } else {
        f32::INFINITY
    };
    let fell_back = prefs.layout == SidePanelLayout::Split && split_max < min_width;
    let effective_layout = if fell_back {
        match prefs.split_fallback {
            SidePanelLayout::Slide => SidePanelLayout::Slide,
            _ => SidePanelLayout::Float,
        }
    } else {
        prefs.layout
    };

    let float_max = if stage_width.is_finite() && stage_width > 0.0 {
        (stage_width - 2.0 * PANEL_FLOAT_INSET).max(min_width)
    } else {
        f32::INFINITY
    };
    let max_width = if effective_layout == SidePanelLayout::Split {
        split_max.max(min_width)
    } else {
        float_max
    };
    let clamped_w = if prefs.width.is_finite() {
        prefs.width.max(min_width)
    } else {
        min_width
    };
    let width = clamped_w.min(max_width);
    let resize_disabled = effective_layout == SidePanelLayout::Split && split_max <= min_width;

    ResolvedPanelLayout {
        effective_layout,
        width,
        max_width,
        fell_back,
        resize_disabled,
    }
}

/// Where one of several open side panels sits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StackedPanelLayout {
    pub layout: ResolvedPanelLayout,
    /// How far a floating or sliding panel sits from its side's edge of the stage: past the split panels on that
    /// side and the floating panels opened before it there. Always 0 for split panels, which take their place in
    /// the row beside the page.
    pub offset: f32,
}

/// Resolves every open side panel, in the order they were opened, so any number can be open at once, as each of the
/// fork's dock frames is independent. `panels` holds each panel's preferences and minimum width.
///
/// Split panels share the stage with the page: each is resolved against the room left after the split panels
/// before it, so a later one falls back to floating when the page would get too narrow. Panels on the same side sit
/// next to each other instead of on top of each other.
pub fn resolve_stacked_panels(
    panels: &[(SidePanelPrefs, f32)],
    stage_width: f32,
    min_pane_share: f32,
) -> Vec<StackedPanelLayout> {
    let mut remaining = stage_width;
    let mut layouts: Vec<ResolvedPanelLayout> = panels
        .iter()
        .map(|(prefs, min_width)| {
            let resolved = resolve_panel_layout_with_min(*prefs, remaining, min_pane_share, *min_width);
            if resolved.effective_layout == SidePanelLayout::Split && remaining.is_finite() {
                remaining -= resolved.width + PANEL_SPLIT_GAP;
            }
            resolved
        })
        .collect();

    // Floating panels on both sides share what the split panels leave. When they do not all fit, each gives up
    // width in proportion to what it has above its minimum; only a window too narrow for every minimum overlaps them.
    if remaining.is_finite() {
        let floating: Vec<usize> = (0..layouts.len())
            .filter(|&i| layouts[i].effective_layout != SidePanelLayout::Split)
            .collect();
        let needed: f32 = floating
            .iter()
            .map(|&i| layouts[i].width + PANEL_FLOAT_INSET)
            .sum::<f32>()
            + PANEL_FLOAT_INSET;
        let excess = needed - remaining;
        let slack: f32 = floating.iter().map(|&i| layouts[i].width - panels[i].1).sum();
        if excess > 0.0 && slack > 0.0 {
            let share = (excess / slack).min(1.0);
            for &i in &floating {
                let give = (layouts[i].width - panels[i].1) * share;
                layouts[i].width = (layouts[i].width - give).floor();
            }
        }
    }

    let split_extent = |side: SidePanelSide| -> f32 {
        panels
            .iter()
            .zip(&layouts)
            .filter(|((prefs, _), layout)| prefs.side == side && layout.effective_layout == SidePanelLayout::Split)
            .map(|(_, layout)| layout.width + PANEL_SPLIT_GAP)
            .sum()
    };
    let split_extents = [split_extent(SidePanelSide::Left), split_extent(SidePanelSide::Right)];
    let mut floating_extent = [0.0_f32; 2];
    panels
        .iter()
        .zip(layouts)
        .map(|((prefs, _), layout)| {
            if layout.effective_layout == SidePanelLayout::Split {
                return StackedPanelLayout { layout, offset: 0.0 };
            }
            let slot = match prefs.side {
                SidePanelSide::Left => 0,
                SidePanelSide::Right => 1,
            };
            let max_offset = if stage_width.is_finite() && stage_width > 0.0 {
                // Float and Slide share identical inset geometry (user decision).
                let inset = 2.0 * PANEL_FLOAT_INSET;
                (stage_width - layout.width - inset).max(0.0)
            } else {
                f32::INFINITY
            };
            let offset = (split_extents[slot] + floating_extent[slot]).min(max_offset);
            floating_extent[slot] += layout.width + PANEL_FLOAT_INSET;
            StackedPanelLayout { layout, offset }
        })
        .collect()
}

/// One selectable context window size offered by a model in the model card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextChoice {
    pub id: String,
    pub label: String,
    pub tokens: u32,
    pub is_default: bool,
}

/// Per-agent user choices stored in profile settings (None means "Default", letting the agent's own setting decide).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentSelection {
    pub model_id: Option<ModelId>,
    pub effort: Option<ModelEffort>,
    pub context_id: Option<String>,
}

/// Distance from each end of the effort slider rail to the thumb's center (`20 px`, matching `ZendiaAgentModelCard.sys.mjs`).
pub const EFFORT_THUMB_INSET: f32 = 20.0;

/// Maps a local horizontal pointer coordinate `x` along an effort slider rail of `width` pixels to `[0.0, 1.0]`.
pub fn effort_rail_fraction(x: f32, width: f32) -> f32 {
    let run = width - 2.0 * EFFORT_THUMB_INSET;
    if run > 0.0 && x.is_finite() {
        ((x - EFFORT_THUMB_INSET) / run).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Snaps a normalized slider fraction `[0.0, 1.0]` to the nearest stop index in `0..count`.
pub fn effort_stop_index(fraction: f32, count: usize) -> usize {
    if count <= 1 {
        return 0;
    }
    (fraction.clamp(0.0, 1.0) * (count - 1) as f32).round() as usize
}

/// Capability tag shown beside a model row in the model list (`(label, is_good)`), matching `ZendiaAgentDock.sys.mjs`.
pub fn model_media_tag(model: &ModelInfo, staged_kinds: &[AttachmentKind]) -> Option<(String, bool)> {
    let media_kinds: Vec<AttachmentKind> = staged_kinds
        .iter()
        .copied()
        .filter(|k| matches!(k, AttachmentKind::Image | AttachmentKind::Video | AttachmentKind::Pdf))
        .collect();
    if !media_kinds.is_empty() {
        let mut unsupported: Vec<&'static str> = Vec::new();
        for kind in media_kinds {
            let (supported, name) = match kind {
                AttachmentKind::Image => (model.supports_images, "images"),
                AttachmentKind::Video => (model.supports_video, "video"),
                AttachmentKind::Pdf => (model.supports_pdf, "PDFs"),
                AttachmentKind::Document => (true, "files"),
            };
            if !supported && !unsupported.contains(&name) {
                unsupported.push(name);
            }
        }
        if unsupported.is_empty() {
            return Some(("reads it".to_string(), true));
        }
        return Some((format!("cannot read {}", unsupported.join(", ")), false));
    }
    if !model.supports_images && !model.supports_video && !model.supports_pdf {
        return Some(("text only".to_string(), false));
    }
    None
}

/// Formats a token count as people read it (`200K`, `1M`).
pub fn format_tokens(tokens: u32) -> String {
    if tokens >= 1_000_000 {
        let tenths = ((tokens as f64 / 100_000.0).round() as u64) as f64 / 10.0;
        if (tenths - tenths.trunc()).abs() < 1e-6 {
            format!("{}M", tenths as u64)
        } else {
            format!("{tenths:.1}M")
        }
    } else {
        format!("{}K", ((tokens as f64) / 1000.0).round() as u64)
    }
}

/// Filters and ranks models for the model card search box: name prefix first, then name substring, then id/detail.
pub fn search_models<'a>(models: &'a [ModelInfo], query: &str) -> Vec<&'a ModelInfo> {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return models.iter().collect();
    }
    let mut ranked: Vec<(u8, usize, &'a ModelInfo)> = models
        .iter()
        .enumerate()
        .filter_map(|(index, model)| {
            let name = model.name.to_ascii_lowercase();
            let rank = if name.starts_with(&needle) {
                0
            } else if name.contains(&needle) {
                1
            } else {
                let hay = format!(
                    "{} {}",
                    model.id.0.to_ascii_lowercase(),
                    model.detail.as_deref().unwrap_or("").to_ascii_lowercase()
                );
                if hay.contains(&needle) { 2 } else { return None }
            };
            Some((rank, index, model))
        })
        .collect();
    ranked.sort_by_key(|(rank, index, _)| (*rank, *index));
    ranked.into_iter().map(|(_, _, model)| model).collect()
}

/// Kind of file attached to an instruction or composer message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentKind {
    Image,
    Video,
    Pdf,
    Document,
}

impl AttachmentKind {
    pub fn from_filename(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        if [".png", ".jpg", ".jpeg", ".gif", ".webp", ".avif", ".bmp", ".svg"]
            .iter()
            .any(|ext| lower.ends_with(ext))
        {
            Self::Image
        } else if [".mp4", ".m4v", ".webm", ".mov", ".ogv"]
            .iter()
            .any(|ext| lower.ends_with(ext))
        {
            Self::Video
        } else if lower.ends_with(".pdf") {
            Self::Pdf
        } else {
            Self::Document
        }
    }

    pub fn extension_badge(name: &str) -> String {
        name.rsplit_once('.')
            .map(|(_, ext)| ext.chars().take(4).collect::<String>().to_ascii_uppercase())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "FILE".to_string())
    }
}

/// Formats a file size in bytes (`512 B`, `18 KB`, `2.4 MB`).
pub fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{} KB", (bytes as f64 / 1024.0).round() as u64)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Describes a mismatch when attached media cannot be read by the currently selected model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaCompatibilityIssue {
    pub model_name: String,
    pub unsupported_kinds: Vec<AttachmentKind>,
    pub suggested_model_id: Option<ModelId>,
    pub suggested_model_name: Option<String>,
}

/// Checks whether `current` can read all attached `kinds` and proposes the first model in `all_models` that can.
pub fn check_media_compatibility(
    current: &ModelInfo,
    all_models: &[ModelInfo],
    kinds: &[AttachmentKind],
) -> Option<MediaCompatibilityIssue> {
    let mut unsupported = Vec::new();
    for kind in kinds {
        let supported = match kind {
            AttachmentKind::Image => current.supports_images,
            AttachmentKind::Video => current.supports_video,
            AttachmentKind::Pdf => current.supports_pdf,
            AttachmentKind::Document => true,
        };
        if !supported && !unsupported.contains(kind) {
            unsupported.push(*kind);
        }
    }
    if unsupported.is_empty() {
        return None;
    }
    let best = all_models.iter().find(|m| {
        unsupported.iter().all(|kind| match kind {
            AttachmentKind::Image => m.supports_images,
            AttachmentKind::Video => m.supports_video,
            AttachmentKind::Pdf => m.supports_pdf,
            AttachmentKind::Document => true,
        })
    });
    Some(MediaCompatibilityIssue {
        model_name: current.name.clone(),
        unsupported_kinds: unsupported,
        suggested_model_id: best.map(|m| m.id.clone()),
        suggested_model_name: best.map(|m| m.name.clone()),
    })
}

/// Stable identifier for a saved agent instruction card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InstructionId(pub u64);

/// File attached to an agent instruction card (at most one per instruction).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstructionAttachment {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub kind: AttachmentKind,
}

/// One guideline card shown in the agent instructions page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentInstruction {
    pub id: InstructionId,
    pub enabled: bool,
    pub title: String,
    pub text: String,
    pub file: Option<InstructionAttachment>,
}

/// Delivery state of a user message sent while the agent is still running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SteerStatus {
    Queued,
    Steering,
    Delivered,
}

/// One row inside an agent-generated interactive table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTableRow {
    pub cells: Vec<String>,
    pub url: Option<String>,
    pub tab_id: Option<TabId>,
}

/// Interactive native table emitted by an agent tool inside the conversation log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTable {
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<AgentTableRow>,
    pub note: Option<String>,
}

/// Current sort state for an [`AgentTable`] view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableSort {
    pub column: usize,
    pub descending: bool,
}

impl TableSort {
    /// Cycles `None -> Ascending -> Descending -> None` when clicking `column`.
    pub fn toggle(current: Option<Self>, column: usize) -> Option<Self> {
        match current {
            Some(sort) if sort.column == column && !sort.descending => Some(Self {
                column,
                descending: true,
            }),
            Some(sort) if sort.column == column => None,
            _ => Some(Self {
                column,
                descending: false,
            }),
        }
    }
}

/// Returns whether a cell is blank or a placeholder dash (`-`, `—`, `–`, `N/A`).
pub fn is_empty_cell(cell: &str) -> bool {
    let t = cell.trim();
    t.is_empty() || matches!(t, "-" | "—" | "–" | "N/A" | "n/a")
}

/// Extracts a sortable number from cells like `"1,240"`, `"$42.50"`, `"88%"`, or `"12 ms"`.
pub fn parse_numeric_cell(cell: &str) -> Option<f64> {
    let trimmed = cell.trim();
    if is_empty_cell(trimmed) {
        return None;
    }
    let cleaned = trimmed
        .trim_start_matches(['$', '€', '£', '¥', '+'])
        .trim_end_matches(['%', 's', 'm', 'K', 'M', 'B', ' '])
        .replace(',', "");
    cleaned.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Returns true when every non-empty cell in `column` parses as a number (and at least one non-empty cell exists).
pub fn is_number_column(table: &AgentTable, column: usize) -> bool {
    let mut saw_value = false;
    for row in &table.rows {
        let cell = row.cells.get(column).map(String::as_str).unwrap_or("");
        if is_empty_cell(cell) {
            continue;
        }
        if parse_numeric_cell(cell).is_none() {
            return false;
        }
        saw_value = true;
    }
    saw_value
}

/// Natural case-insensitive string comparison (`"item2"` < `"item10"`), matching JS `localeCompare(..., { numeric: true })`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ac), Some(bc)) if ac.is_ascii_digit() && bc.is_ascii_digit() => {
                let mut an = String::new();
                while let Some(&c) = ai.peek() {
                    if c.is_ascii_digit() {
                        an.push(c);
                        ai.next();
                    } else {
                        break;
                    }
                }
                let mut bn = String::new();
                while let Some(&c) = bi.peek() {
                    if c.is_ascii_digit() {
                        bn.push(c);
                        bi.next();
                    } else {
                        break;
                    }
                }
                let at = an.trim_start_matches('0');
                let bt = bn.trim_start_matches('0');
                let ord = at
                    .len()
                    .cmp(&bt.len())
                    .then_with(|| at.cmp(bt))
                    .then_with(|| an.len().cmp(&bn.len()));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(ac), Some(bc)) => {
                ai.next();
                bi.next();
                let ord = ac.to_ascii_lowercase().cmp(&bc.to_ascii_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
            }
        }
    }
}

/// Filters `table` rows by substring query and applies optional column sorting (keeping empty cells last).
pub fn filter_and_sort_rows<'a>(
    table: &'a AgentTable,
    filter: &str,
    sort: Option<TableSort>,
) -> Vec<&'a AgentTableRow> {
    let needle = filter.trim().to_ascii_lowercase();
    let mut indexed: Vec<(usize, &'a AgentTableRow)> = table
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            needle.is_empty() || row.cells.iter().any(|cell| cell.to_ascii_lowercase().contains(&needle))
        })
        .collect();

    if let Some(sort) = sort
        && sort.column < table.columns.len()
    {
        let numeric = is_number_column(table, sort.column);
        indexed.sort_by(|(idx_a, row_a), (idx_b, row_b)| {
            let cell_a = row_a.cells.get(sort.column).map(String::as_str).unwrap_or("");
            let cell_b = row_b.cells.get(sort.column).map(String::as_str).unwrap_or("");
            let empty_a = is_empty_cell(cell_a);
            let empty_b = is_empty_cell(cell_b);
            if empty_a || empty_b {
                return empty_a.cmp(&empty_b).then_with(|| idx_a.cmp(idx_b));
            }
            let ord = if numeric {
                let a = parse_numeric_cell(cell_a).unwrap_or(0.0);
                let b = parse_numeric_cell(cell_b).unwrap_or(0.0);
                a.partial_cmp(&b).unwrap_or(Ordering::Equal)
            } else {
                natural_cmp(cell_a, cell_b)
            };
            let directed = if sort.descending { ord.reverse() } else { ord };
            directed.then_with(|| idx_a.cmp(idx_b))
        });
    }

    indexed.into_iter().map(|(_, row)| row).collect()
}

fn csv_escape(cell: &str) -> String {
    if cell.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
    }
}

/// Serializes visible table columns and rows as RFC 4180 CSV for "Copy CSV".
pub fn table_to_csv(columns: &[String], rows: &[&AgentTableRow]) -> String {
    let mut lines = Vec::with_capacity(rows.len() + 1);
    lines.push(columns.iter().map(|c| csv_escape(c)).collect::<Vec<_>>().join(","));
    for row in rows {
        let cells: Vec<String> = (0..columns.len())
            .map(|i| csv_escape(row.cells.get(i).map(String::as_str).unwrap_or("")))
            .collect();
        lines.push(cells.join(","));
    }
    lines.join("\n")
}

/// Native declared UI page emitted by an agent using `zendia-module-api`'s [`UiNode`] tree.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentDeclaredPage {
    pub title: String,
    pub draft: bool,
    pub root: UiNode,
}

// ---------------------------------------------------------------------------
// Conversation follow-scroll & working motion math (pure, unit-tested)
// ---------------------------------------------------------------------------

const FOLLOW_DAMPING: f32 = 0.7;
const FOLLOW_STIFFNESS: f32 = 0.05;
const FOLLOW_MASS: f32 = 1.25;
const FOLLOW_LEAD_FRAMES: f32 = 9.0;
const FOLLOW_MAX_LEAD_PX: f32 = 32.0;

/// Spring state for conversation auto-follow scrolling.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FollowState {
    pub position: f32,
    pub velocity: f32,
    pub growth: f32,
}

/// Advances the conversation follow spring by `frames` (at 60 fps, `<= 1.0` per sub-step).
pub fn follow_step(state: FollowState, target: f32, frames: f32) -> FollowState {
    let chase = target - (state.growth * FOLLOW_LEAD_FRAMES).min(FOLLOW_MAX_LEAD_PX);
    let distance = (chase - state.position).max(0.0);
    let next_velocity = state.velocity
        + frames * ((FOLLOW_DAMPING * state.velocity + FOLLOW_STIFFNESS * distance) / FOLLOW_MASS - state.velocity);
    FollowState {
        position: (state.position + (next_velocity + state.growth) * frames).min(target),
        velocity: next_velocity,
        growth: state.growth,
    }
}

/// Computes the runway height held under a newly sent user prompt so it rests at the top of the viewport.
pub fn runway_height(view_height: f32, content_height: f32, lane_height: f32) -> f32 {
    lane_height.max(view_height - content_height + lane_height)
}

/// Active run duration of the working ring (`3.9 s`).
pub const RING_RUN_SECS: f32 = 3.9;
/// Rest interval between working ring runs (`0.9 s`).
pub const RING_REST_SECS: f32 = 0.9;
const RING_DASHES: usize = 8;
const RING_HALF_DEG: f32 = 10.95;

/// Geometry and highlight intensity for one dash of Zendia's 8-dash working ring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RingDash {
    pub centre_deg: f32,
    pub half_deg: f32,
    pub glow: f32,
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn wrap180(degrees: f32) -> f32 {
    ((((degrees + 180.0) % 360.0) + 360.0) % 360.0) - 180.0
}

/// Pure-Rust deterministic sine (avoids platform-dependent libm `f32::sin`).
pub fn sin_f32(x: f32) -> f32 {
    let two_pi = 2.0 * PI;
    let mut a = x % two_pi;
    if a > PI {
        a -= two_pi;
    } else if a < -PI {
        a += two_pi;
    }
    if a > PI * 0.5 {
        a = PI - a;
    } else if a < -PI * 0.5 {
        a = -PI - a;
    }
    let a2 = a * a;
    a * (1.0 - a2 / 6.0 * (1.0 - a2 / 20.0 * (1.0 - a2 / 42.0)))
}

/// Pure-Rust deterministic cosine (avoids platform-dependent libm `f32::cos`).
pub fn cos_f32(x: f32) -> f32 {
    sin_f32(x + PI * 0.5)
}

/// Pure-Rust deterministic negative exponential `e^x` for `x <= 0.0` (used for smooth Gaussian glow).
fn exp_neg_f32(x: f32) -> f32 {
    if x >= 0.0 {
        return 1.0;
    }
    if x < -16.0 {
        return 0.0;
    }
    // Range reduction: e^x = (e^(x / 16))^16 with a 5-term Taylor series on |t| <= 1.
    let t = x / 16.0;
    let mut y = 1.0 + t * (1.0 + t * (0.5 + t * (1.0 / 6.0 + t * (1.0 / 24.0 + t / 120.0))));
    y *= y; // ^2
    y *= y; // ^4
    y *= y; // ^8
    y *= y; // ^16
    y.clamp(0.0, 1.0)
}

/// Computes the 8 dashes of the working ring at `phase_secs` inside the `3.9 s` run + `0.9 s` rest cycle.
pub fn ring_dashes(phase_secs: f32) -> [RingDash; RING_DASHES] {
    let cycle = RING_RUN_SECS + RING_REST_SECS;
    let ph = phase_secs.rem_euclid(cycle);
    let running = ph > 0.0 && ph < RING_RUN_SECS;
    let tau = if running { ph / RING_RUN_SECS } else { 0.0 };
    let lead = 720.0 * (0.5 - 0.5 * cos_f32(PI * tau));
    let speed = sin_f32(PI * tau);
    let amp = if running {
        smoothstep(0.0, 0.55, ph) * (1.0 - smoothstep(RING_RUN_SECS - 0.75, RING_RUN_SECS, ph))
    } else {
        0.0
    };
    let pull = lead - 14.0 * speed;

    std::array::from_fn(|i| {
        let centre = (360.0 / RING_DASHES as f32) * i as f32;
        let dl = wrap180(pull - centre);
        let sigma = if dl < 0.0 { 26.0 } else { 40.0 };
        let glow = amp * exp_neg_f32(-(dl * dl) / (sigma * sigma));
        let sign = if dl > 0.0 {
            1.0
        } else if dl < 0.0 {
            -1.0
        } else {
            0.0
        };
        RingDash {
            centre_deg: centre + sign * 9.0 * glow,
            half_deg: RING_HALF_DEG * (1.0 + 1.3 * glow),
            glow,
        }
    })
}

const WORKING_WORDS: &[&str] = &[
    "Thinking",
    "Reading",
    "Pondering",
    "Tinkering",
    "Yanking",
    "Sifting",
    "Weaving",
    "Plotting",
    "Composing",
    "Untangling",
    "Distilling",
    "Sketching",
    "Buffering",
    "Hopping",
    "Threading",
    "Musing",
];

const WORD_ROTATE_SECS: u64 = 7;

/// Stable seed hash so a conversation keeps its working word sequence.
pub fn seed_of(text: &str) -> usize {
    let mut n: usize = 0;
    for byte in text.bytes() {
        n = (n * 31 + byte as usize) % 9973;
    }
    n
}

/// Returns the working status word for `seed` at `elapsed_secs`, rotating every 7 seconds.
pub fn working_word(seed: usize, elapsed_secs: u64) -> String {
    let step = (elapsed_secs / WORD_ROTATE_SECS) as usize;
    format!("{}…", WORKING_WORDS[(seed + step) % WORKING_WORDS.len()])
}

/// Returns `(common_prefix_chars, common_suffix_chars)` of two strings without overlapping, for per-glyph text rolling.
pub fn shared_ends(a: &str, b: &str) -> (usize, usize) {
    let left: Vec<char> = a.chars().collect();
    let right: Vec<char> = b.chars().collect();
    let shortest = left.len().min(right.len());
    let mut prefix = 0;
    while prefix < shortest && left[prefix] == right[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < shortest - prefix && left[left.len() - 1 - suffix] == right[right.len() - 1 - suffix] {
        suffix += 1;
    }
    (prefix, suffix)
}

/// Streaming veil opacity curve `1 - (1 - p)^1.6` for `p` in `0.0..=1.0`.
pub fn veil_alpha(progress: f32) -> f32 {
    let p = progress.clamp(0.0, 1.0);
    let q = 1.0 - p;
    // q^1.5 = q * sqrt(q), closely matching q^1.6 without libm `powf`.
    let q_pow = q * q.sqrt();
    (1.0 - q_pow).clamp(0.0, 1.0)
}

/// Smoothed arrival gap (ms) for streaming chunks.
pub fn next_ema_ms(ema: f32, gap_ms: f32) -> f32 {
    ema * 0.7 + gap_ms.min(1000.0) * 0.3
}

/// Fade duration (ms) for a newly arrived streaming chunk, clamped to `120..=400 ms`.
pub fn chunk_duration_ms(ema: f32) -> f32 {
    (ema * 3.0).clamp(120.0, 400.0)
}

/// Speed multiplier when several streaming veil chunks overlap.
pub fn veil_speed(active_chunks: usize) -> f32 {
    if active_chunks >= 3 {
        1.0 + 0.3 * (active_chunks - 2) as f32
    } else {
        1.0
    }
}

/// One newly streamed text range currently fading in via the streaming veil (`AM-02`).
#[derive(Clone, Debug, PartialEq)]
pub struct VeilChunk {
    pub start_char: usize,
    pub end_char: usize,
    pub duration_ms: f32,
    pub elapsed_ms: f32,
}

impl VeilChunk {
    pub fn alpha(&self) -> f32 {
        if self.duration_ms <= 0.0 {
            return 1.0;
        }
        veil_alpha((self.elapsed_ms / self.duration_ms).clamp(0.0, 1.0))
    }
}

/// Pure per-message streaming veil tracker matching `ZendiaAgentMotion.sys.mjs`'s `Veil`.
#[derive(Clone, Debug, PartialEq)]
pub struct VeilTracker {
    text: String,
    pub chunks: Vec<VeilChunk>,
    pub ema_ms: f32,
    pub speed: f32,
    last_chunk_ms: Option<f32>,
    last_sync_ms: f32,
    initialized: bool,
}

impl Default for VeilTracker {
    fn default() -> Self {
        Self {
            text: String::new(),
            chunks: Vec::new(),
            ema_ms: 80.0,
            speed: 1.0,
            last_chunk_ms: None,
            last_sync_ms: 0.0,
            initialized: false,
        }
    }
}

impl VeilTracker {
    /// Records the current streamed text at `now_ms`, creating a fading chunk for any newly appended range.
    /// If `baseline` is true (or on first observation), existing text is recorded at full opacity without dissolving.
    pub fn update(&mut self, text: &str, now_ms: f32, baseline: bool) {
        if !self.initialized || baseline {
            self.text = text.to_string();
            self.chunks.clear();
            self.ema_ms = 80.0;
            self.speed = 1.0;
            self.last_chunk_ms = None;
            self.last_sync_ms = now_ms;
            self.initialized = true;
            return;
        }

        let dt = (now_ms - self.last_sync_ms).max(0.0);
        let common = self.text.chars().zip(text.chars()).take_while(|(a, b)| a == b).count();
        let new_len = text.chars().count();

        let mut kept = Vec::with_capacity(self.chunks.len() + 1);
        for mut chunk in self.chunks.drain(..) {
            chunk.elapsed_ms += dt * self.speed;
            if chunk.start_char < common && chunk.elapsed_ms < chunk.duration_ms {
                chunk.end_char = chunk.end_char.min(common);
                if chunk.start_char < chunk.end_char {
                    kept.push(chunk);
                }
            }
        }

        if new_len > common {
            if let Some(prev) = self.last_chunk_ms {
                self.ema_ms = next_ema_ms(self.ema_ms, (now_ms - prev).max(0.0));
            }
            self.last_chunk_ms = Some(now_ms);
            kept.push(VeilChunk {
                start_char: common,
                end_char: new_len,
                duration_ms: chunk_duration_ms(self.ema_ms),
                elapsed_ms: 0.0,
            });
        }

        self.text = text.to_string();
        self.chunks = kept;
        self.speed = veil_speed(self.chunks.len());
        self.last_sync_ms = now_ms;
    }

    /// Advances active chunk fade timers to `now_ms` and prunes completed chunks.
    pub fn tick(&mut self, now_ms: f32) {
        if self.chunks.is_empty() {
            self.last_sync_ms = now_ms;
            return;
        }
        let dt = (now_ms - self.last_sync_ms).max(0.0);
        let speed = self.speed;
        self.chunks.retain_mut(|chunk| {
            chunk.elapsed_ms += dt * speed;
            chunk.elapsed_ms < chunk.duration_ms
        });
        self.speed = veil_speed(self.chunks.len());
        self.last_sync_ms = now_ms;
    }

    pub fn has_active_chunks(&self) -> bool {
        !self.chunks.is_empty()
    }

    /// Returns the veil opacity (`0.0..=1.0`) for a character offset `char_idx` inside the tracked text.
    pub fn alpha_at(&self, char_idx: usize) -> f32 {
        for chunk in &self.chunks {
            if char_idx >= chunk.start_char && char_idx < chunk.end_char {
                return chunk.alpha();
            }
        }
        1.0
    }
}

/// Formats a duration in seconds for the working row and folded step group (`1s`, `1m 14s`).
pub fn format_duration_secs(secs: u64) -> String {
    let clamped = secs.max(1);
    if clamped < 60 {
        format!("{clamped}s")
    } else {
        format!("{}m {}s", clamped / 60, clamped % 60)
    }
}

/// Formats elapsed seconds as `m:ss` for the closed dock pill (`0:12`, `2:05`).
pub fn format_elapsed_clock(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// Formats a relative timestamp for finished/idle roster rows (`7s ago`, `3m ago`, `2h ago`).
pub fn format_relative_age(secs: u64) -> String {
    if secs < 60 {
        format!("{}s ago", secs.max(1))
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h ago", secs / 3600)
    }
}

/// Roster section bucket in the Agents panel, ordered by user urgency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RosterGroup {
    NeedsYou,
    Working,
    Finished,
    Idle,
}

impl RosterGroup {
    pub const ALL: [Self; 4] = [Self::NeedsYou, Self::Working, Self::Finished, Self::Idle];

    pub fn heading(self) -> &'static str {
        match self {
            Self::NeedsYou => "NEEDS YOU",
            Self::Working => "WORKING",
            Self::Finished => "FINISHED",
            Self::Idle => "IDLE",
        }
    }
}

/// Detailed 12-state roster classification matching `ZendiaAgentRoster.sys.mjs` (`AG-04`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RosterKind {
    Confirm,
    Captcha,
    Blocked,
    Paused,
    Error,
    Stopping,
    CaptchaSolving,
    Working,
    Starting,
    Done,
    Stopped,
    Idle,
}

/// Derived roster presentation state for one conversation row (`AG-04`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RosterEntryState {
    pub group: RosterGroup,
    pub kind: RosterKind,
    pub label: String,
    pub detail: String,
}

/// Strips basic Markdown formatting and collapses whitespace into a single clamped line (`ZendiaAgentRoster.oneLine`).
pub fn strip_one_line(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut prev_space = true;
    for ch in text.chars() {
        let mapped = match ch {
            '\n' | '\r' | '\t' => ' ',
            '*' | '`' | '~' => continue,
            c => c,
        };
        if mapped.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(mapped);
            prev_space = false;
        }
    }
    let trimmed = out.trim();
    let count = trimmed.chars().count();
    if count > max_chars && max_chars > 1 {
        let mut cut: String = trimmed.chars().take(max_chars - 1).collect();
        cut = cut.trim_end().to_string();
        cut.push('…');
        cut
    } else {
        trimmed.to_string()
    }
}

fn last_answer_line(steps: &[crate::model::ConversationStep]) -> String {
    for step in steps.iter().rev() {
        match &step.kind {
            crate::model::StepKind::UserPrompt => return String::new(),
            crate::model::StepKind::Response if !step.summary.trim().is_empty() => {
                return strip_one_line(&step.summary, 140);
            }
            _ => {}
        }
    }
    String::new()
}

/// Derives the rich 12-state roster status for `conv` (`AG-04`).
pub fn derive_roster_state(conv: &crate::model::Conversation, captcha_holds: bool) -> RosterEntryState {
    if let Some(prompt) = conv.steps.iter().rev().find_map(|step| match &step.kind {
        crate::model::StepKind::Confirmation { resolved: None, prompt } => Some(prompt.as_str()),
        _ => None,
    }) {
        return RosterEntryState {
            group: RosterGroup::NeedsYou,
            kind: RosterKind::Confirm,
            label: "Asks permission".to_string(),
            detail: strip_one_line(prompt, 140),
        };
    }

    if conv.status == crate::model::ConversationStatus::Working {
        let running_step = conv
            .steps
            .iter()
            .rev()
            .take_while(|s| !matches!(s.kind, crate::model::StepKind::UserPrompt))
            .find(|s| s.status == crate::model::StepStatus::Running);
        let has_activity = conv
            .steps
            .iter()
            .rev()
            .take_while(|s| !matches!(s.kind, crate::model::StepKind::UserPrompt))
            .next()
            .is_some();
        let detail = running_step
            .map(|s| strip_one_line(&s.summary, 140))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                let ans = last_answer_line(&conv.steps);
                if ans.is_empty() {
                    conv.steps
                        .iter()
                        .rev()
                        .find(|s| !matches!(s.kind, crate::model::StepKind::UserPrompt))
                        .map(|s| strip_one_line(&s.summary, 140))
                        .unwrap_or_default()
                } else {
                    ans
                }
            });
        let starting = !has_activity && conv.elapsed_secs == 0;
        return RosterEntryState {
            group: RosterGroup::Working,
            kind: if starting {
                RosterKind::Starting
            } else {
                RosterKind::Working
            },
            label: if starting { "Starting" } else { "Working" }.to_string(),
            detail,
        };
    }

    if captcha_holds {
        return RosterEntryState {
            group: RosterGroup::NeedsYou,
            kind: RosterKind::Captcha,
            label: "CAPTCHA to solve".to_string(),
            detail: "It carries on once the CAPTCHA is solved".to_string(),
        };
    }

    // Check for an error step since the latest user prompt (when not stopped manually without error).
    if let Some(err_step) = conv
        .steps
        .iter()
        .rev()
        .take_while(|s| !matches!(s.kind, crate::model::StepKind::UserPrompt))
        .find(|s| matches!(s.kind, crate::model::StepKind::Error))
    {
        return RosterEntryState {
            group: RosterGroup::NeedsYou,
            kind: RosterKind::Error,
            label: "Stopped with an error".to_string(),
            detail: strip_one_line(&err_step.summary, 140),
        };
    }

    if conv.status == crate::model::ConversationStatus::WaitingForUser {
        return RosterEntryState {
            group: RosterGroup::NeedsYou,
            kind: RosterKind::Blocked,
            label: "Blocked on you".to_string(),
            detail: last_answer_line(&conv.steps),
        };
    }

    if conv.status == crate::model::ConversationStatus::Stopped {
        return RosterEntryState {
            group: RosterGroup::Idle,
            kind: RosterKind::Stopped,
            label: "Stopped".to_string(),
            detail: last_answer_line(&conv.steps),
        };
    }

    let answer = last_answer_line(&conv.steps);
    if conv.status == crate::model::ConversationStatus::Finished && conv.unseen {
        return RosterEntryState {
            group: RosterGroup::Finished,
            kind: RosterKind::Done,
            label: "Finished".to_string(),
            detail: answer,
        };
    }

    RosterEntryState {
        group: RosterGroup::Idle,
        kind: RosterKind::Idle,
        label: "Idle".to_string(),
        detail: answer,
    }
}

/// Classifies a conversation into its Agents panel roster bucket.
pub fn classify_roster_group(conv: &crate::model::Conversation) -> RosterGroup {
    derive_roster_state(conv, false).group
}

/// Kind of entry shown inside a conversation's Agents panel peek drawer (`AG-07`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RosterActivityKind {
    Step,
    Agent,
    Note,
    Error,
}

/// One activity entry rendered in the Agents panel peek drawer (up to 6 items, `AG-07`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RosterActivityItem {
    pub kind: RosterActivityKind,
    pub status: crate::model::StepStatus,
    pub text: String,
}

/// Extracts the last `max_items` mixed activity entries after the latest user prompt (`ZendiaAgentRoster.recentActivity`).
pub fn recent_roster_activity(steps: &[crate::model::ConversationStep], max_items: usize) -> Vec<RosterActivityItem> {
    let start = steps
        .iter()
        .rposition(|s| matches!(s.kind, crate::model::StepKind::UserPrompt))
        .map_or(0, |i| i + 1);
    let mut items = Vec::new();
    for step in &steps[start..] {
        match &step.kind {
            crate::model::StepKind::ToolCall { .. } | crate::model::StepKind::Thinking => {
                items.push(RosterActivityItem {
                    kind: RosterActivityKind::Step,
                    status: step.status,
                    text: strip_one_line(&step.summary, 100),
                });
            }
            crate::model::StepKind::Response => {
                if !step.summary.trim().is_empty() {
                    items.push(RosterActivityItem {
                        kind: RosterActivityKind::Agent,
                        status: step.status,
                        text: strip_one_line(&step.summary, 220),
                    });
                }
            }
            crate::model::StepKind::AgentTable(table) => {
                items.push(RosterActivityItem {
                    kind: RosterActivityKind::Note,
                    status: step.status,
                    text: strip_one_line(&table.title, 100),
                });
            }
            crate::model::StepKind::DeclaredPage(page) => {
                items.push(RosterActivityItem {
                    kind: RosterActivityKind::Note,
                    status: step.status,
                    text: strip_one_line(&page.title, 100),
                });
            }
            crate::model::StepKind::Note => {
                if !step.summary.trim().is_empty() {
                    items.push(RosterActivityItem {
                        kind: RosterActivityKind::Note,
                        status: step.status,
                        text: strip_one_line(&step.summary, 140),
                    });
                }
            }
            crate::model::StepKind::Error => {
                items.push(RosterActivityItem {
                    kind: RosterActivityKind::Error,
                    status: crate::model::StepStatus::Failed,
                    text: strip_one_line(&step.summary, 140),
                });
            }
            crate::model::StepKind::Confirmation { prompt, .. } => {
                items.push(RosterActivityItem {
                    kind: RosterActivityKind::Note,
                    status: step.status,
                    text: strip_one_line(prompt, 140),
                });
            }
            crate::model::StepKind::UserPrompt => {}
        }
    }
    if items.len() > max_items {
        items.drain(..items.len() - max_items);
    }
    items
}

/// Segmented work-list progress counts (`done`, `other`/failed, `total`) for the `3 px` Agents roster bar (`AG-06`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkListProgress {
    pub done: usize,
    pub other: usize,
    pub total: usize,
}

/// Computes segmented step progress for the latest turn when at least two tool/thinking steps exist (`AG-06`).
pub fn work_list_progress(steps: &[crate::model::ConversationStep]) -> Option<WorkListProgress> {
    let start = steps
        .iter()
        .rposition(|s| matches!(s.kind, crate::model::StepKind::UserPrompt))
        .map_or(0, |i| i + 1);
    let mut done = 0usize;
    let mut other = 0usize;
    let mut total = 0usize;
    for step in &steps[start..] {
        if matches!(
            step.kind,
            crate::model::StepKind::ToolCall { .. } | crate::model::StepKind::Thinking
        ) {
            total += 1;
            match step.status {
                crate::model::StepStatus::Completed => done += 1,
                crate::model::StepStatus::Failed => other += 1,
                _ => {}
            }
        }
    }
    (total >= 2).then_some(WorkListProgress { done, other, total })
}

/// Aggregate counts of conversations across the four Agents panel buckets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RosterCounts {
    pub needs_you: usize,
    pub working: usize,
    pub finished: usize,
    pub idle: usize,
}

impl RosterCounts {
    pub fn from_conversations(conversations: &[crate::model::Conversation]) -> Self {
        let mut counts = Self::default();
        for conv in conversations {
            match classify_roster_group(conv) {
                RosterGroup::NeedsYou => counts.needs_you += 1,
                RosterGroup::Working => counts.working += 1,
                RosterGroup::Finished => counts.finished += 1,
                RosterGroup::Idle => counts.idle += 1,
            }
        }
        counts
    }

    pub fn total(&self) -> usize {
        self.needs_you + self.working + self.finished + self.idle
    }
}

/// Computes the count badge shown on the Agents toolbar button (`TB-02`): hidden while the Agents panel is open,
/// amber (`Warning`) when any agent waits on the user, accent when agents are working, capped at `"9+"`.
pub fn agents_button_badge_for_state(
    model: &crate::model::BrowserModel,
    agents_panel_open: bool,
) -> Option<crate::contributions::ToolbarBadge> {
    if agents_panel_open {
        return None;
    }
    let server_running = model.services.iter().any(|s| {
        s.service == crate::model::ServiceId::AgentServer && matches!(s.status, crate::model::ServiceStatus::Running)
    });
    if !server_running {
        return None;
    }
    let counts = RosterCounts::from_conversations(&model.conversations);
    let format_count = |n: usize| if n > 9 { "9+".to_string() } else { n.to_string() };
    if counts.needs_you > 0 {
        Some(crate::contributions::ToolbarBadge {
            text: format_count(counts.needs_you),
            tone: crate::contributions::BadgeTone::Warning,
        })
    } else if counts.working > 0 {
        Some(crate::contributions::ToolbarBadge {
            text: format_count(counts.working),
            tone: crate::contributions::BadgeTone::Accent,
        })
    } else {
        None
    }
}

/// Computes the count badge shown on the Agents toolbar button assuming the panel is closed.
pub fn agents_button_badge(model: &crate::model::BrowserModel) -> Option<crate::contributions::ToolbarBadge> {
    agents_button_badge_for_state(model, false)
}

/// Dynamic tooltip for the Agents toolbar button matching `ZendiaAgentsButton.sys.mjs:64-79` (`TB-02`).
pub fn agents_button_tooltip(model: &crate::model::BrowserModel, agents_panel_open: bool) -> String {
    if agents_panel_open {
        return "Close agents".to_string();
    }
    let mut waiting = Vec::new();
    let mut working = 0usize;
    for conv in &model.conversations {
        let captcha = conv
            .tab_id
            .is_some_and(|tid| model.tab_captcha_findings.contains_key(&tid));
        let st = derive_roster_state(conv, captcha);
        match st.group {
            RosterGroup::NeedsYou => {
                let name = model
                    .agent(&conv.agent_id)
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| "An agent".to_string());
                waiting.push((name, st.label));
            }
            RosterGroup::Working => working += 1,
            _ => {}
        }
    }
    if waiting.len() == 1 {
        let (name, lbl) = &waiting[0];
        format!("{name}: {}. Open agents", lbl.to_ascii_lowercase())
    } else if !waiting.is_empty() {
        format!("{} agents need you. Open agents", waiting.len())
    } else if working == 1 {
        "1 agent working. Open agents".to_string()
    } else if working > 1 {
        format!("{working} agents working. Open agents")
    } else {
        "All your agents".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(side: SidePanelSide, layout: SidePanelLayout, width: f32) -> (SidePanelPrefs, f32) {
        (
            SidePanelPrefs {
                side,
                layout,
                width,
                ..SidePanelPrefs::default()
            },
            PANEL_MIN_WIDTH,
        )
    }

    #[test]
    fn panels_on_both_sides_open_together() {
        let stacked = resolve_stacked_panels(
            &[
                panel(SidePanelSide::Right, SidePanelLayout::Float, 420.0),
                panel(SidePanelSide::Left, SidePanelLayout::Float, 440.0),
            ],
            1400.0,
            1.0,
        );
        assert_eq!(stacked.len(), 2);
        assert_eq!(stacked[0].offset, 0.0);
        assert_eq!(stacked[1].offset, 0.0);
        assert_eq!(stacked[1].layout.width, 440.0);
    }

    #[test]
    fn floating_panels_on_one_side_sit_next_to_each_other() {
        let stacked = resolve_stacked_panels(
            &[
                panel(SidePanelSide::Right, SidePanelLayout::Split, 440.0),
                panel(SidePanelSide::Right, SidePanelLayout::Float, 460.0),
                panel(SidePanelSide::Right, SidePanelLayout::Float, 480.0),
            ],
            1800.0,
            1.0,
        );
        assert_eq!(stacked[0].layout.effective_layout, SidePanelLayout::Split);
        assert_eq!(stacked[1].offset, 440.0 + PANEL_SPLIT_GAP);
        assert_eq!(stacked[2].offset, 440.0 + PANEL_SPLIT_GAP + 460.0 + PANEL_FLOAT_INSET);
    }

    #[test]
    fn floating_panels_shrink_toward_their_minimum_to_fit_the_stage() {
        let stacked = resolve_stacked_panels(
            &[
                panel(SidePanelSide::Left, SidePanelLayout::Float, 520.0),
                panel(SidePanelSide::Right, SidePanelLayout::Float, 520.0),
            ],
            1000.0,
            1.0,
        );
        let total: f32 = stacked.iter().map(|p| p.layout.width + PANEL_FLOAT_INSET).sum::<f32>() + PANEL_FLOAT_INSET;
        assert!(total <= 1000.0, "{total}");
        assert!(stacked.iter().all(|p| p.layout.width >= PANEL_MIN_WIDTH));

        let roomy = resolve_stacked_panels(
            &[panel(SidePanelSide::Left, SidePanelLayout::Float, 520.0)],
            1600.0,
            1.0,
        );
        assert_eq!(roomy[0].layout.width, 520.0);
    }

    #[test]
    fn a_later_split_panel_floats_when_the_page_would_get_too_narrow() {
        let stacked = resolve_stacked_panels(
            &[
                panel(SidePanelSide::Right, SidePanelLayout::Split, 420.0),
                panel(SidePanelSide::Left, SidePanelLayout::Split, 420.0),
            ],
            1100.0,
            1.0,
        );
        assert_eq!(stacked[0].layout.effective_layout, SidePanelLayout::Split);
        assert!(stacked[1].layout.fell_back);
        assert_eq!(stacked[1].layout.effective_layout, SidePanelLayout::Float);
        assert_eq!(stacked[1].offset, 0.0);
    }

    #[test]
    fn split_layout_falls_back_when_stage_is_too_narrow() {
        let prefs = SidePanelPrefs {
            layout: SidePanelLayout::Split,
            side: SidePanelSide::Right,
            width: 460.0,
            split_fallback: SidePanelLayout::Float,
        };
        let wide = resolve_panel_layout(prefs, 1200.0, 1.0);
        assert_eq!(wide.effective_layout, SidePanelLayout::Split);
        assert!(!wide.fell_back);
        assert_eq!(wide.width, 460.0);

        // 780 px stage cannot fit 420 px panel + 8 px gap + 420 px page.
        let narrow = resolve_panel_layout(prefs, 780.0, 1.0);
        assert_eq!(narrow.effective_layout, SidePanelLayout::Float);
        assert!(narrow.fell_back);
    }

    #[test]
    fn float_and_slide_resolve_to_identical_width_max_width_and_offset() {
        // Single panel across roomy and narrow stages (including webchat and agents widths).
        for stage in [1600.0_f32, 900.0, 440.0] {
            for (width, min_w) in [
                (PANEL_DEFAULT_WIDTH, PANEL_MIN_WIDTH),
                (WEBCHAT_PANEL_DEFAULT_WIDTH, PANEL_MIN_WIDTH),
                (AGENTS_PANEL_DEFAULT_WIDTH, AGENTS_PANEL_MIN_WIDTH),
            ] {
                for side in [SidePanelSide::Left, SidePanelSide::Right] {
                    let float_p = (
                        SidePanelPrefs {
                            layout: SidePanelLayout::Float,
                            side,
                            width,
                            ..SidePanelPrefs::default()
                        },
                        min_w,
                    );
                    let slide_p = (
                        SidePanelPrefs {
                            layout: SidePanelLayout::Slide,
                            side,
                            width,
                            ..SidePanelPrefs::default()
                        },
                        min_w,
                    );
                    let f = resolve_stacked_panels(&[float_p], stage, 1.0);
                    let s = resolve_stacked_panels(&[slide_p], stage, 1.0);
                    assert_eq!(f[0].layout.width, s[0].layout.width);
                    assert_eq!(f[0].layout.max_width, s[0].layout.max_width);
                    assert_eq!(f[0].offset, s[0].offset);
                }
            }
        }

        // Split falling back to Float vs Slide resolves to the exact same geometry.
        let split_to_float = SidePanelPrefs {
            layout: SidePanelLayout::Split,
            side: SidePanelSide::Right,
            width: 480.0,
            split_fallback: SidePanelLayout::Float,
        };
        let split_to_slide = SidePanelPrefs {
            split_fallback: SidePanelLayout::Slide,
            ..split_to_float
        };
        let rf = resolve_panel_layout(split_to_float, 780.0, 1.0);
        let rs = resolve_panel_layout(split_to_slide, 780.0, 1.0);
        assert!(rf.fell_back && rs.fell_back);
        assert_eq!(rf.effective_layout, SidePanelLayout::Float);
        assert_eq!(rs.effective_layout, SidePanelLayout::Slide);
        assert_eq!(rf.width, rs.width);
        assert_eq!(rf.max_width, rs.max_width);

        // Multiple stacked panels on the same side (both roomy and narrow where max_offset clamps).
        for stage in [1600.0_f32, 900.0] {
            let float_stack = resolve_stacked_panels(
                &[
                    panel(SidePanelSide::Right, SidePanelLayout::Float, 480.0),
                    panel(SidePanelSide::Right, SidePanelLayout::Float, 420.0),
                    (
                        SidePanelPrefs {
                            side: SidePanelSide::Left,
                            layout: SidePanelLayout::Float,
                            width: AGENTS_PANEL_DEFAULT_WIDTH,
                            ..SidePanelPrefs::default()
                        },
                        AGENTS_PANEL_MIN_WIDTH,
                    ),
                ],
                stage,
                1.0,
            );
            let slide_stack = resolve_stacked_panels(
                &[
                    panel(SidePanelSide::Right, SidePanelLayout::Slide, 480.0),
                    panel(SidePanelSide::Right, SidePanelLayout::Slide, 420.0),
                    (
                        SidePanelPrefs {
                            side: SidePanelSide::Left,
                            layout: SidePanelLayout::Slide,
                            width: AGENTS_PANEL_DEFAULT_WIDTH,
                            ..SidePanelPrefs::default()
                        },
                        AGENTS_PANEL_MIN_WIDTH,
                    ),
                ],
                stage,
                1.0,
            );
            for (f, s) in float_stack.iter().zip(&slide_stack) {
                assert_eq!(f.layout.width, s.layout.width);
                assert_eq!(f.layout.max_width, s.layout.max_width);
                assert_eq!(f.offset, s.offset);
            }
        }
    }

    #[test]
    fn table_sort_filter_and_csv_work_on_numeric_and_text_columns() {
        let table = AgentTable {
            title: "Benchmarks".to_string(),
            columns: vec!["Name".to_string(), "Latency".to_string()],
            rows: vec![
                AgentTableRow {
                    cells: vec!["Beta, v2".to_string(), "120 ms".to_string()],
                    url: None,
                    tab_id: None,
                },
                AgentTableRow {
                    cells: vec!["Alpha".to_string(), "15 ms".to_string()],
                    url: Some("https://example.org".to_string()),
                    tab_id: None,
                },
                AgentTableRow {
                    cells: vec!["Gamma".to_string(), "—".to_string()],
                    url: None,
                    tab_id: None,
                },
            ],
            note: None,
        };
        assert!(!is_number_column(&table, 0));
        assert!(is_number_column(&table, 1));

        let sorted = filter_and_sort_rows(
            &table,
            "",
            Some(TableSort {
                column: 1,
                descending: false,
            }),
        );
        // Numeric ascending, empty cell ("—") last.
        assert_eq!(sorted[0].cells[0], "Alpha");
        assert_eq!(sorted[1].cells[0], "Beta, v2");
        assert_eq!(sorted[2].cells[0], "Gamma");

        let csv = table_to_csv(&table.columns, &sorted);
        assert_eq!(csv, "Name,Latency\nAlpha,15 ms\n\"Beta, v2\",120 ms\nGamma,—");
    }

    #[test]
    fn search_models_ranks_prefix_before_substring_and_detail() {
        let models = vec![
            ModelInfo::simple(
                ModelId::new("claude-haiku-4-5"),
                "Claude Haiku 4.5",
                "Anthropic",
                200_000,
                ModelEffort::Low,
            ),
            ModelInfo::simple(
                ModelId::new("sonnet-fast"),
                "Sonnet 4.6",
                "Anthropic",
                1_000_000,
                ModelEffort::High,
            ),
            ModelInfo::simple(
                ModelId::new("claude-sonnet-4-6"),
                "Claude Sonnet 4.6",
                "Anthropic",
                1_000_000,
                ModelEffort::High,
            ),
        ];
        let hits = search_models(&models, "sonnet");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].name, "Sonnet 4.6");
        assert_eq!(hits[1].name, "Claude Sonnet 4.6");
    }

    #[test]
    fn motion_ring_follow_scroll_and_shared_ends_match_spec() {
        let rest = ring_dashes(RING_RUN_SECS + 0.2);
        assert!(rest.iter().all(|d| d.glow == 0.0));
        let active = ring_dashes(1.2);
        assert!(active.iter().any(|d| d.glow > 0.2));

        let next = follow_step(
            FollowState {
                position: 0.0,
                velocity: 0.0,
                growth: 2.0,
            },
            400.0,
            1.0,
        );
        assert!(next.position > 0.0 && next.position < 400.0);
        assert_eq!(runway_height(600.0, 120.0, 32.0), 512.0);
        assert_eq!(shared_ends("Thinking…", "Tinkering…"), (1, 4));
        assert_eq!(format_tokens(200_000), "200K");
        assert_eq!(format_tokens(1_000_000), "1M");
        assert_eq!(format_duration_secs(74), "1m 14s");
        assert_eq!(format_elapsed_clock(65), "1:05");
    }

    #[test]
    fn effort_rail_fraction_and_media_tags_match_fork() {
        // Rail width 232px => thumb inset 20px, run = 192px.
        assert_eq!(effort_rail_fraction(0.0, 232.0), 0.0);
        assert_eq!(effort_rail_fraction(20.0, 232.0), 0.0);
        assert!((effort_rail_fraction(116.0, 232.0) - 0.5).abs() < 1e-4);
        assert_eq!(effort_rail_fraction(212.0, 232.0), 1.0);
        assert_eq!(effort_rail_fraction(260.0, 232.0), 1.0);
        assert_eq!(effort_stop_index(0.0, 4), 0);
        assert_eq!(effort_stop_index(0.34, 4), 1);
        assert_eq!(effort_stop_index(0.67, 4), 2);
        assert_eq!(effort_stop_index(1.0, 4), 3);

        let text_only = ModelInfo {
            supports_images: false,
            supports_video: false,
            supports_pdf: false,
            ..ModelInfo::simple(
                ModelId::new("haiku"),
                "Claude Haiku",
                "Anthropic",
                200_000,
                ModelEffort::Low,
            )
        };
        assert_eq!(model_media_tag(&text_only, &[]), Some(("text only".to_string(), false)));
        assert_eq!(
            model_media_tag(&text_only, &[AttachmentKind::Image]),
            Some(("cannot read images".to_string(), false))
        );
        let vision = ModelInfo {
            supports_images: true,
            ..text_only
        };
        assert_eq!(model_media_tag(&vision, &[]), None);
        assert_eq!(
            model_media_tag(&vision, &[AttachmentKind::Image]),
            Some(("reads it".to_string(), true))
        );
    }

    #[test]
    fn natural_cmp_orders_embedded_numbers_numerically() {
        assert_eq!(natural_cmp("item2", "item10"), Ordering::Less);
        assert_eq!(natural_cmp("Phase 10", "Phase 2"), Ordering::Greater);
        assert_eq!(natural_cmp("alpha", "ALPHA"), Ordering::Equal);
    }

    #[test]
    fn veil_tracker_fades_only_newly_appended_ranges() {
        let mut tracker = VeilTracker::default();
        tracker.update("Hello", 0.0, false);
        assert!(!tracker.has_active_chunks());
        assert_eq!(tracker.alpha_at(0), 1.0);

        tracker.update("Hello world", 50.0, false);
        assert!(tracker.has_active_chunks());
        assert_eq!(tracker.alpha_at(0), 1.0);
        assert_eq!(tracker.alpha_at(4), 1.0);
        assert!(tracker.alpha_at(6) < 0.05);

        tracker.tick(170.0);
        assert!(tracker.alpha_at(6) > 0.3);

        tracker.tick(600.0);
        assert!(!tracker.has_active_chunks());
        assert_eq!(tracker.alpha_at(6), 1.0);
    }

    #[test]
    fn roster_state_progress_activity_and_badge_match_fork() {
        use crate::model::{
            AgentId, Conversation, ConversationId, ConversationStatus, ConversationStep, StepId, StepKind, StepStatus,
        };

        assert_eq!(AGENTS_PANEL_MIN_WIDTH, 280.0);
        assert_eq!(SidePanelPrefs::min_width_for(AGENTS_PANEL_ID), 280.0);
        assert_eq!(SidePanelPrefs::default_width_for(AGENTS_PANEL_ID), 340.0);

        let err_conv = Conversation {
            id: ConversationId(1),
            agent_id: AgentId::new("claude-code"),
            tab_id: None,
            title: "Broken run".to_string(),
            model_id: ModelId::new("claude-sonnet-4"),
            status: ConversationStatus::Stopped,
            steps: vec![
                ConversationStep::simple(StepId(1), StepKind::UserPrompt, "Fix bug", StepStatus::Completed),
                ConversationStep::simple(StepId(2), StepKind::Error, "Network timeout", StepStatus::Failed),
            ],
            unseen: false,
            elapsed_secs: 4,
        };
        let st = derive_roster_state(&err_conv, false);
        assert_eq!(st.group, RosterGroup::NeedsYou);
        assert_eq!(st.kind, RosterKind::Error);
        assert_eq!(st.label, "Stopped with an error");

        let captcha_conv = Conversation {
            status: ConversationStatus::Idle,
            steps: vec![ConversationStep::simple(
                StepId(1),
                StepKind::UserPrompt,
                "Check",
                StepStatus::Completed,
            )],
            ..err_conv.clone()
        };
        let cap_st = derive_roster_state(&captcha_conv, true);
        assert_eq!(cap_st.group, RosterGroup::NeedsYou);
        assert_eq!(cap_st.kind, RosterKind::Captcha);

        let starting_conv = Conversation {
            status: ConversationStatus::Working,
            elapsed_secs: 0,
            ..captcha_conv.clone()
        };
        assert_eq!(derive_roster_state(&starting_conv, false).kind, RosterKind::Starting);

        let multi_step = vec![
            ConversationStep::simple(StepId(1), StepKind::UserPrompt, "Go", StepStatus::Completed),
            ConversationStep::simple(
                StepId(2),
                StepKind::ToolCall { tool: "a".into() },
                "Step 1",
                StepStatus::Completed,
            ),
            ConversationStep::simple(
                StepId(3),
                StepKind::ToolCall { tool: "b".into() },
                "Step 2",
                StepStatus::Failed,
            ),
            ConversationStep::simple(
                StepId(4),
                StepKind::ToolCall { tool: "c".into() },
                "Step 3",
                StepStatus::Running,
            ),
        ];
        assert_eq!(
            work_list_progress(&multi_step),
            Some(WorkListProgress {
                done: 1,
                other: 1,
                total: 3
            })
        );
        assert_eq!(recent_roster_activity(&multi_step, 6).len(), 3);

        // Badge hides when open and caps >9 at "9+".
        let mut model = crate::stub::sample_model(true);
        model.conversations.clear();
        for i in 0..12u64 {
            model.conversations.push(Conversation {
                id: ConversationId(100 + i),
                status: ConversationStatus::Working,
                elapsed_secs: 3,
                ..captcha_conv.clone()
            });
        }
        assert_eq!(agents_button_badge_for_state(&model, true), None);
        let badge = agents_button_badge_for_state(&model, false).expect("badge expected when closed");
        assert_eq!(badge.text, "9+");
    }
}
