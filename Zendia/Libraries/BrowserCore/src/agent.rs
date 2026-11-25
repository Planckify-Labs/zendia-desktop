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
/// Default width of the agents roster side panel (`340 px`).
pub const AGENTS_PANEL_DEFAULT_WIDTH: f32 = 340.0;
/// Minimum logical width of the agents roster side panel (`320 px`).
pub const AGENTS_PANEL_MIN_WIDTH: f32 = 320.0;
/// Default and minimum logical width of the agent side panel (`420 px`).
pub const PANEL_MIN_WIDTH: f32 = 420.0;
/// Maximum logical width of a side panel (`820 px`).
pub const PANEL_MAX_WIDTH: f32 = 820.0;
/// Default width of the agent side panel (`420 px`).
pub const PANEL_DEFAULT_WIDTH: f32 = 420.0;
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

    pub fn clamp_width_for(panel_id: &str, width: f32) -> f32 {
        let min_w = Self::min_width_for(panel_id);
        if width.is_finite() {
            width.max(min_w)
        } else if panel_id == AGENTS_PANEL_ID {
            AGENTS_PANEL_DEFAULT_WIDTH
        } else {
            PANEL_DEFAULT_WIDTH
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
                cell_a.to_ascii_lowercase().cmp(&cell_b.to_ascii_lowercase())
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

/// Classifies a conversation into its Agents panel roster bucket.
pub fn classify_roster_group(conv: &crate::model::Conversation) -> RosterGroup {
    let has_pending_confirm = conv
        .steps
        .iter()
        .any(|step| matches!(step.kind, crate::model::StepKind::Confirmation { resolved: None, .. }));
    if conv.status == crate::model::ConversationStatus::WaitingForUser || has_pending_confirm {
        RosterGroup::NeedsYou
    } else if conv.status == crate::model::ConversationStatus::Working {
        RosterGroup::Working
    } else if conv.status == crate::model::ConversationStatus::Finished && conv.unseen {
        RosterGroup::Finished
    } else {
        RosterGroup::Idle
    }
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

/// Computes the count badge shown on the Agents toolbar button (`Warning` amber when any agent waits on the user).
pub fn agents_button_badge(model: &crate::model::BrowserModel) -> Option<crate::contributions::ToolbarBadge> {
    let server_running = model.services.iter().any(|s| {
        s.service == crate::model::ServiceId::AgentServer && matches!(s.status, crate::model::ServiceStatus::Running)
    });
    if !server_running {
        return None;
    }
    let counts = RosterCounts::from_conversations(&model.conversations);
    if counts.needs_you > 0 {
        Some(crate::contributions::ToolbarBadge {
            text: counts.needs_you.to_string(),
            tone: crate::contributions::BadgeTone::Warning,
        })
    } else if counts.working + counts.finished > 0 {
        Some(crate::contributions::ToolbarBadge {
            text: (counts.working + counts.finished).to_string(),
            tone: crate::contributions::BadgeTone::Accent,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
