/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The client side of the Automation World, the privileged isolated world inside each page.
//!
//! It finds, clicks, scrolls, selects, and reads elements, and gives them stable references. Vim hints, visual mode,
//! field navigation, CAPTCHA solving, and AI agents all use the same Automation World and element references. Requests
//! reach the page through the single generic embedder channel (`view_embedder_message` / `view_embedder_reply`).
//!
//! See `Zendia/Documentation/Architecture.md`, "Features".

use serde::{Deserialize, Serialize};

/// The isolated world identifier used inside `WebContent` for Zendia's Automation World.
pub const AUTOMATION_WORLD_NAME: &str = "zendia-automation";

/// The JavaScript runtime installed once per document inside the Automation World.
pub const AUTOMATION_WORLD_JS: &str = include_str!("automation_world.js");

/// What a jump labels on the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JumpKind {
    /// Every control but the fields, labeled at once (`s`).
    Hints,
    /// Controls by some of their text, then a label (`gs`).
    Controls,
    /// The places a typed text occurs on the page, then a label (`S`).
    Text,
    /// Every paragraph and word, to grow the visual selection to one (`s` in visual mode).
    Words,
    /// Every paragraph and word, to say where visual mode starts (`v`).
    Land,
    /// The page's fields, when `i` finds several.
    Inputs,
    /// The controls of the page's CAPTCHA (`gc`, `<leader>cc`).
    Captcha,
    /// Only the active CAPTCHA challenge's controls (when `s` transitions into a staying jump on a challenge pick).
    Challenge,
}

impl JumpKind {
    /// Whether every target is labeled at once, so keys only pick labels. Otherwise typed characters narrow the
    /// targets to those that contain them before labels appear.
    pub fn labels_at_once(self) -> bool {
        !matches!(self, Self::Controls | Self::Text)
    }
}

/// A motion of visual mode's cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualMotion {
    Left,
    Right,
    Down,
    Up,
    WordForward,
    WordBackward,
    LineStart,
    LineEnd,
    ParagraphForward,
    ParagraphBackward,
    Top,
    Bottom,
}

/// A scroll of the page, of the element the page scrolls in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollMotion {
    Down,
    Up,
    Left,
    Right,
    HalfPageDown,
    HalfPageUp,
    PageDown,
    PageUp,
    Top,
    Bottom,
}

/// The controls a CAPTCHA has, pressed by what they do (`<leader>cv`, `<leader>ci`, `<leader>cr`, `<leader>ca`,
/// `<leader>cp`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptchaOp {
    Verify,
    Answer,
    Reload,
    Audio,
    Play,
}

/// A rectangle in the page's viewport, in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewportRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// One thing a jump can go to, named by an id that stays the same while the page keeps it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageTarget {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// `control`, `field`, `match`, `word` or `paragraph`.
    #[serde(default)]
    pub kind: String,
    /// The characters that follow each match of the typed text, which no label may be.
    #[serde(default)]
    pub nexts: Vec<String>,
    /// How far it is from where the user is looking: the middle of the page, or the visual mode cursor.
    #[serde(default)]
    pub distance: f32,
    #[serde(default)]
    pub href: Option<String>,
    /// Whether this target belongs to a CAPTCHA challenge (so picking it in `s` or `gc` keeps the jump open).
    #[serde(default)]
    pub challenge: bool,
    /// Whether this CAPTCHA control is dragged rather than clicked (slider handle, puzzle piece, press-and-hold).
    #[serde(default)]
    pub drag: bool,
    /// Whether this CAPTCHA control slides horizontally along a track.
    #[serde(default)]
    pub slider: bool,
    /// The track box a slider handle slides along, in viewport CSS pixels.
    #[serde(default)]
    pub track: Option<ViewportRect>,
    /// Whether this target is a CAPTCHA picture or canvas to open the keyboard cursor over.
    #[serde(default)]
    pub picture: bool,
    /// What role this control plays inside a CAPTCHA (`verify`, `answer`, `reload`, `audio`, `visual`, `play`).
    #[serde(default)]
    pub purpose: Option<String>,
}

impl PageTarget {
    pub fn rect(&self) -> ViewportRect {
        ViewportRect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

/// A single CAPTCHA detection signal reported by a frame (`ZendiaCaptchaWatchChild.scanPage`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptchaWatchDetection {
    pub signal: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub challenge: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub detail: Option<String>,
}

/// Live CAPTCHA state reported by a frame (`ZendiaCaptchaWatchChild.captchaState`).
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CaptchaWatchStateReading {
    #[serde(default = "default_state_none")]
    pub state: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub how: Option<String>,
}

fn default_state_none() -> String {
    "none".to_string()
}

/// A request from the browser core to a page's Automation World.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AutomationRequest {
    /// Installs the Automation World runtime into the document so focus, scroll, and yank listeners are active early.
    Init {
        #[serde(default)]
        watch_mutations: bool,
    },
    /// The targets of a jump; for [`JumpKind::Controls`] and [`JumpKind::Text`], those that match `pattern`.
    JumpScan {
        jump: JumpKind,
        #[serde(default)]
        pattern: String,
        /// Scan only the top document without recursing into child frames (`EXT-01` popup hints).
        #[serde(default)]
        top_only: bool,
        /// Include editable fields alongside other controls in [`JumpKind::Hints`] (`EXT-01` popup hints).
        #[serde(default)]
        include_fields: bool,
        /// Observe DOM mutations in this document and push `page_moved` events (`EXT-01` popup hints).
        #[serde(default)]
        watch_mutations: bool,
    },
    /// Goes to a target of the last scan. `extend` grows the visual selection to a text target.
    JumpPick {
        id: String,
        #[serde(default)]
        new_tab: bool,
        #[serde(default)]
        extend: bool,
    },
    Scroll {
        motion: ScrollMotion,
        #[serde(default = "default_count")]
        count: u32,
    },
    /// `i`: the one field, the field a count names, or labels on several.
    InputChoose {
        #[serde(default)]
        count: Option<u32>,
    },
    InputFirst {
        #[serde(default)]
        count: Option<u32>,
    },
    InputLast {
        #[serde(default)]
        count: Option<u32>,
    },
    InputStep {
        delta: i32,
    },
    /// The focused field gives up the focus.
    BlurField,
    /// `<C-u>`: deletes backward to the start of the current line without deleting a preceding line break.
    EditDeleteToLineStart,
    /// Deletes forward to the end of the current line without deleting a trailing line break.
    EditDeleteToLineEnd,
    /// Deletes one character forward in the focused field.
    EditDeleteCharForward,
    VisualEnter,
    VisualExit,
    VisualYank,
    /// Copies the active selection with full HTML and plain-text representations (`Ctrl+C`, `VIS-02`).
    CopySelection,
    /// Cuts the active editable selection via the page's clipboard handler (`Ctrl+X`, `YNK-02`).
    CutSelection,
    VisualSwapEnds,
    VisualMove {
        motion: VisualMotion,
        #[serde(default = "default_count")]
        count: u32,
    },
    /// A text object, named by the keys that ran it (`aw`, `i{`, `a<lt>`).
    VisualTextObject {
        keys: String,
        #[serde(default = "default_count")]
        count: u32,
    },
    /// Where the visual mode cursor is now.
    Caret,
    /// Puts a yank in the focused field; `replace` puts it over the put the caret sits after.
    YankPut {
        text: String,
        index: usize,
        #[serde(default)]
        replace: bool,
    },
    /// Which yank `<A-p>` (`step` 1) or `<A-n>` (`step` -1) puts next.
    YankCycle {
        step: i32,
    },
    CaptchaAction {
        action: CaptchaOp,
    },
    /// Queries the bounding areas of recognised CAPTCHAs on screen (`captchaAreas`) and solved token count.
    CaptchaAreas,
    /// Forces a fresh `ZendiaCaptchaWatchChild` scan across the top document and all child frames.
    CaptchaWatchScan,
    /// Fills a local CAPTCHA answer field with `text` as user input (`captcha.fill`).
    CaptchaFill {
        text: String,
    },
    /// Dispatches a synthetic touch gesture stage (`start`, `move`, `end`) alongside mouse drag events (`CAP-05`, `CAP-06`).
    DispatchTouch {
        stage: String,
        x: f32,
        y: f32,
        #[serde(default)]
        reset_chain: bool,
    },
    /// Blurs any focused `iframe` in the top document and focuses the top window so keys stay on the page (`CAP-07`).
    RefocusTop,
    /// Toggles clean reader view on the current document (`<leader>uz`).
    ToggleReaderMode,
}

fn default_count() -> u32 {
    1
}

fn default_true() -> bool {
    true
}

/// An answer from the Automation World.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AutomationResponse {
    Targets {
        jump: JumpKind,
        #[serde(default)]
        pattern: String,
        targets: Vec<PageTarget>,
        /// Count of solved CAPTCHA token fields and visible solved widget marks on the page (`CAP-02`, `CAP-03`).
        #[serde(default)]
        captcha_tokens: u32,
        /// Recognised CAPTCHA bounding boxes on screen when scanning a CAPTCHA jump (`CAP-05`).
        #[serde(default)]
        captcha_areas: Vec<ViewportRect>,
        /// Top viewport `(innerWidth, innerHeight)` in CSS pixels.
        #[serde(default)]
        viewport: Option<(f32, f32)>,
    },
    /// Answer to [`AutomationRequest::CaptchaAreas`].
    CaptchaAreas {
        areas: Vec<ViewportRect>,
        #[serde(default)]
        captcha_tokens: u32,
        #[serde(default)]
        viewport: Option<(f32, f32)>,
    },
    /// A field took the focus (`mode` `i`), or a link is to open in a new tab (`url`).
    Activated {
        #[serde(default)]
        mode: Option<String>,
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        message: Option<String>,
    },
    /// A control to press where it is on top, at a point of the viewport in CSS pixels (`ctrl` holds Ctrl for capital hints, `JMP-03`).
    Press {
        x: f32,
        y: f32,
        #[serde(default)]
        ctrl: bool,
    },
    VisualState {
        active: bool,
        #[serde(default)]
        yanked: Option<String>,
        #[serde(default)]
        copied_by_page: bool,
        #[serde(default)]
        caret: Option<ViewportRect>,
        /// `v` left the cursor collapsed: labels say where it lands.
        #[serde(default)]
        land: bool,
        /// What a yank took, to flash.
        #[serde(default)]
        flash: Vec<ViewportRect>,
    },
    /// No field to type in: insert mode until `<Esc>`, as with `I`.
    ExplicitInsert,
    Put {
        ok: bool,
        text: String,
        #[serde(default)]
        flash: Vec<ViewportRect>,
    },
    Cycle {
        index: usize,
    },
    /// Unsolicited or queried report of whether an editable element currently holds focus in the page.
    EditableFocus {
        editing: bool,
    },
    /// Unsolicited report that the page or one of its scrollers scrolled or resized.
    PageMoved {
        #[serde(default)]
        scrolled: bool,
    },
    /// Unsolicited report of text copied or cut from a non-password context in the page.
    PageYank {
        text: String,
        #[serde(default)]
        host: String,
    },
    /// Unsolicited CAPTCHA watch report from one frame of a tab (`ZendiaCaptchaWatchChild`).
    CaptchaWatchReport {
        #[serde(default)]
        frame_id: u64,
        #[serde(default = "default_true")]
        top: bool,
        #[serde(default)]
        url: String,
        #[serde(default)]
        detections: Vec<CaptchaWatchDetection>,
        #[serde(default)]
        state: CaptchaWatchStateReading,
    },
    /// Unsolicited notice that a frame left or unloaded (`Zendia:CaptchaWatch:Gone`).
    CaptchaWatchGone {
        #[serde(default)]
        frame_id: u64,
    },
    /// Result of toggling reader view (`<leader>uz`).
    ReaderMode {
        active: bool,
        #[serde(default)]
        message: Option<String>,
    },
    /// The runtime is not in this document yet: the request goes again with it.
    NeedRuntime,
    Message {
        text: String,
    },
    Ok,
}

/// The script that runs `request` in the Automation World, installing the runtime first when `with_runtime` is
/// set. Without it, a document that has no runtime yet answers [`AutomationResponse::NeedRuntime`], so the runtime
/// goes over the channel once per document instead of with every key.
pub fn build_eval_script(request: &AutomationRequest, with_runtime: bool) -> Result<String, serde_json::Error> {
    let request_json = serde_json::to_string(request)?;
    let escaped_request = serde_json::to_string(&request_json)?;
    let runtime = if with_runtime { AUTOMATION_WORLD_JS } else { "" };
    Ok(format!(
        "(function(){{{runtime}\nif(typeof globalThis.__zendiaAutomation!=='function'){{return '{{\"kind\":\"need_runtime\"}}';}}return globalThis.__zendiaAutomation({escaped_request});}})()"
    ))
}

/// Parses a JSON string returned by the Automation World.
pub fn parse_response(json: &str) -> Result<AutomationResponse, serde_json::Error> {
    serde_json::from_str(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_and_response_round_trip_json() {
        let req = AutomationRequest::VisualTextObject {
            keys: "i{".to_string(),
            count: 2,
        };
        let script = build_eval_script(&req, true).unwrap();
        assert!(script.contains("__zendiaAutomation"));
        assert!(script.contains("visual_text_object"));

        let resp_json = r#"{"kind":"targets","jump":"hints","targets":[{"id":"c1","x":10.0,"y":20.0,"width":80.0,"height":24.0,"kind":"control","nexts":[],"distance":3.5,"href":"https://example.com"}]}"#;
        let resp = parse_response(resp_json).unwrap();
        match resp {
            AutomationResponse::Targets { jump, targets, .. } => {
                assert_eq!(jump, JumpKind::Hints);
                assert_eq!(targets.len(), 1);
                assert_eq!(targets[0].id, "c1");
                assert_eq!(targets[0].kind, "control");
            }
            other => panic!("unexpected response: {other:?}"),
        }
        assert_eq!(
            parse_response(r#"{"kind":"need_runtime"}"#).unwrap(),
            AutomationResponse::NeedRuntime
        );
        assert!(
            !build_eval_script(&AutomationRequest::Caret, false)
                .unwrap()
                .contains("deepElements")
        );
    }
}
