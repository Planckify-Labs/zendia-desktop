/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The vim commands that act inside the page through the Automation World: visual mode (`visual.*`), jump labels
//! (`jump.*`), the page's fields (`input.*`) and the CAPTCHA's controls (`captcha.*`).
//!
//! A jump is a [`PageHintSession`] the core owns and the UI draws over the page. The page only finds targets and
//! acts on the one picked, so a page can neither hide nor fake the labels. The session follows the old Zendia's
//! jumps:
//! - Labels at once ([`JumpKind::labels_at_once`]: `s`, `i` with several fields, the words of visual mode): every
//!   target gets an equal-length label from [`crate::focus::HINT_ALPHABET`], in reading order. Typing narrows to the
//!   labels that start with what was typed; a key that starts none does nothing. A capital letter opens a link in a
//!   new tab.
//! - Labels after text (`gs`, `S`): typing narrows the targets to those whose text holds what was typed, and only
//!   then are they labeled, one letter each from [`JUMP_LABELS`], nearest first. A label is never a character that
//!   follows a match, so a key is a label or more text, never both, and a target keeps its label as the text grows.
//! - `<CR>` takes the first target, `<BS>` takes back a key (or ends the jump), `<Esc>` ends it, and a key with Ctrl,
//!   Alt or Super ends it and goes on to the keymap.

use crate::commands::{Command, CommandDescriptor, CommandError};
use crate::focus::hint_labels;
use crate::model::TabId;
use std::collections::{BTreeMap, VecDeque};
use zendia_automation::{CaptchaOp, JumpKind, PageTarget, ViewportRect, VisualMotion};

/// The letters of the labels that appear once text was typed, home row first.
pub const JUMP_LABELS: &str = "asdfghjklqwertyuiopzxcvbnm";

/// One target of a jump, as the UI draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct PageHintItem {
    pub id: String,
    /// `None` when the labels ran out, or before any text was typed in a jump that labels after text.
    pub label: Option<String>,
    pub rect: ViewportRect,
    /// `control`, `field`, `match`, `word` or `paragraph`.
    pub kind: String,
    pub href: Option<String>,
}

/// A jump in progress on one tab's page.
#[derive(Clone, Debug, PartialEq)]
pub struct PageHintSession {
    pub tab_id: TabId,
    pub kind: JumpKind,
    /// The targets in the order they are labeled.
    pub items: Vec<PageHintItem>,
    /// The label keys typed so far, when every target is labeled at once.
    pub typed: String,
    /// The text typed so far, when labels come after text.
    pub pattern: String,
    /// Whether a pick of text grows the visual selection rather than starting it there.
    pub extend: bool,
    /// The text the page is being asked about, while its answer is on the way.
    pub awaiting: Option<String>,
    /// Keys typed while the page was being asked, handled in order once it answers.
    pub queued: VecDeque<HintKey>,
    /// The label each target had, which it keeps while it is free.
    previous: BTreeMap<String, String>,
}

/// A key given to a jump.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HintKey {
    /// A printable character, as typed (a capital letter keeps its case).
    Char(char),
    Enter,
    Backspace,
    Escape,
    Space,
    /// A key with Ctrl, Alt or Super: the jump ends and the key goes to the keymap.
    Chord,
    /// Any other key, which a jump has no use for.
    Other,
}

/// What a key did to a jump.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HintOutcome {
    /// A target was picked: the jump ends and the page goes to it.
    Pick { id: String, new_tab: bool },
    /// The page is to be asked for the targets of this text.
    Ask { pattern: String },
    /// The labels changed.
    Changed,
    /// The jump ended.
    Finished,
    /// The jump ended, and the key goes on to the keymap.
    FinishedPassingKey,
    /// Nothing happened.
    Ignored,
}

impl PageHintSession {
    pub fn new(tab_id: TabId, kind: JumpKind, extend: bool) -> Self {
        Self {
            tab_id,
            kind,
            items: Vec::new(),
            typed: String::new(),
            pattern: String::new(),
            extend,
            awaiting: None,
            queued: VecDeque::new(),
            previous: BTreeMap::new(),
        }
    }

    /// Takes the targets the page found for `pattern` and labels them.
    pub fn set_targets(&mut self, mut targets: Vec<PageTarget>) {
        // Nearest to where the user looks first; a stable sort keeps the page's order between equals.
        targets.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        if self.kind.labels_at_once() {
            // Then in reading order: rows of 8 pixels, top to bottom, then left to right.
            let row = |target: &PageTarget| (target.y / 8.0 + 0.5).floor() as i64;
            targets.sort_by(|a, b| row(a).cmp(&row(b)).then(a.x.total_cmp(&b.x)));
            let labels = hint_labels(targets.len());
            self.items = targets
                .into_iter()
                .zip(labels)
                .map(|(target, label)| item(target, Some(label)))
                .collect();
            return;
        }
        let labels = if self.pattern.is_empty() {
            vec![None; targets.len()]
        } else {
            let skip: Vec<char> = targets
                .iter()
                .flat_map(|target| target.nexts.iter().filter_map(|next| next.chars().next()))
                .collect();
            let previous: Vec<Option<String>> = targets
                .iter()
                .map(|target| self.previous.get(&target.id).cloned())
                .collect();
            assign_labels(targets.len(), &skip, &previous)
        };
        self.previous = targets
            .iter()
            .zip(&labels)
            .filter_map(|(target, label)| label.clone().map(|label| (target.id.clone(), label)))
            .collect();
        self.items = targets
            .into_iter()
            .zip(labels)
            .map(|(target, label)| item(target, label))
            .collect();
    }

    /// The targets drawn now: with labels at once, those whose label starts with what was typed.
    pub fn visible_items(&self) -> impl Iterator<Item = &PageHintItem> {
        self.items.iter().filter(|item| {
            !self.kind.labels_at_once()
                || item
                    .label
                    .as_deref()
                    .is_some_and(|label| label.starts_with(&self.typed))
        })
    }

    /// Whether the jump labels text (the prompt shows what was typed).
    pub fn shows_prompt(&self) -> bool {
        !self.kind.labels_at_once()
    }

    /// Feeds one key. While the page is being asked about some text, keys wait their turn.
    pub fn feed(&mut self, key: HintKey) -> HintOutcome {
        if key == HintKey::Escape {
            return HintOutcome::Finished;
        }
        if self.awaiting.is_some() {
            self.queued.push_back(key);
            return HintOutcome::Ignored;
        }
        match key {
            HintKey::Escape => HintOutcome::Finished,
            HintKey::Chord => HintOutcome::FinishedPassingKey,
            HintKey::Enter => match self.items.first() {
                Some(first) => HintOutcome::Pick {
                    id: first.id.clone(),
                    new_tab: false,
                },
                None => HintOutcome::Finished,
            },
            HintKey::Backspace => self.backspace(),
            HintKey::Space if !self.kind.labels_at_once() => self.type_char(' '),
            HintKey::Char(ch) => self.type_char(ch),
            HintKey::Space | HintKey::Other => HintOutcome::Ignored,
        }
    }

    /// The page answered for `pattern`. Returns `false` when the text matched nothing, so the key that asked does
    /// nothing.
    pub fn answer(&mut self, pattern: &str, targets: Vec<PageTarget>) -> bool {
        if self.awaiting.as_deref() != Some(pattern) {
            return false;
        }
        self.awaiting = None;
        // Taking back the last character always works; more text must match something.
        if targets.is_empty() && !pattern.is_empty() {
            return false;
        }
        self.pattern = pattern.to_string();
        self.set_targets(targets);
        true
    }

    fn backspace(&mut self) -> HintOutcome {
        if self.kind.labels_at_once() {
            if self.typed.pop().is_some() {
                return HintOutcome::Changed;
            }
            return HintOutcome::Finished;
        }
        if self.pattern.is_empty() {
            return HintOutcome::Finished;
        }
        let mut shorter = self.pattern.clone();
        shorter.pop();
        self.ask(shorter)
    }

    fn type_char(&mut self, ch: char) -> HintOutcome {
        let lower: String = ch.to_lowercase().collect();
        let new_tab = lower != ch.to_string();
        if self.kind.labels_at_once() {
            let typed = format!("{}{lower}", self.typed);
            if let Some(item) = self
                .items
                .iter()
                .find(|item| item.label.as_deref() == Some(typed.as_str()))
            {
                return HintOutcome::Pick {
                    id: item.id.clone(),
                    new_tab,
                };
            }
            if self
                .items
                .iter()
                .any(|item| item.label.as_deref().is_some_and(|label| label.starts_with(&typed)))
            {
                self.typed = typed;
                return HintOutcome::Changed;
            }
            return HintOutcome::Ignored;
        }
        // A label wins over a character of the text: no label is a character that follows a match.
        if !self.pattern.is_empty()
            && let Some(item) = self
                .items
                .iter()
                .find(|item| item.label.as_deref() == Some(lower.as_str()))
        {
            return HintOutcome::Pick {
                id: item.id.clone(),
                new_tab,
            };
        }
        self.ask(format!("{}{ch}", self.pattern))
    }

    fn ask(&mut self, pattern: String) -> HintOutcome {
        self.awaiting = Some(pattern.clone());
        HintOutcome::Ask { pattern }
    }
}

fn item(target: PageTarget, label: Option<String>) -> PageHintItem {
    PageHintItem {
        rect: target.rect(),
        id: target.id,
        label,
        kind: target.kind,
        href: target.href,
    }
}

/// Labels for `count` targets from [`JUMP_LABELS`], never one of `skip` (the characters that may follow a match),
/// each target keeping its `previous` label while it is free. `None` where the letters ran out.
pub fn assign_labels(count: usize, skip: &[char], previous: &[Option<String>]) -> Vec<Option<String>> {
    let blocked: Vec<char> = skip.iter().flat_map(|ch| ch.to_lowercase()).collect();
    let mut free: Vec<char> = JUMP_LABELS.chars().filter(|ch| !blocked.contains(ch)).collect();
    let mut labels = vec![None; count];
    for (i, label) in previous.iter().enumerate().take(count) {
        if let Some(label) = label
            && let Some(ch) = label.chars().next()
            && let Some(position) = free.iter().position(|free| *free == ch)
        {
            labels[i] = Some(label.clone());
            free.remove(position);
        }
    }
    let mut rest = free.into_iter();
    for label in labels.iter_mut() {
        if label.is_none() {
            match rest.next() {
                Some(ch) => *label = Some(ch.to_string()),
                None => break,
            }
        }
    }
    labels
}

/// Vim commands that act in the page.
#[derive(Clone, Debug, PartialEq)]
pub enum PageAction {
    /// `v`: visual mode on the page's text, with labels on where to land.
    VisualEnter,
    /// Leaves visual mode; the cursor stays where the selection ended.
    VisualExit,
    /// `y`: copies the selection into the clipboard and the yank ring, and leaves visual mode.
    VisualYank,
    /// `o`: the other end of the selection.
    VisualSwapEnds,
    VisualMove {
        motion: VisualMotion,
        count: Option<u32>,
    },
    /// A text object, named by the keys that ran it (`aw`, `i{`, `a<lt>`).
    VisualObject {
        keys: String,
        count: Option<u32>,
    },
    /// Labels on the page (`s`, `gs`, `S`, `gc`, the words of visual mode).
    Jump {
        kind: JumpKind,
    },
    /// A key typed while a jump has the keys.
    PageHintInput {
        key: HintKey,
    },
    /// The page moved under a jump: its targets are found again.
    PageHintRefresh,
    PageHintCancel,
    /// `i`: the field to type in, by label when there are several.
    InputChoose {
        count: Option<u32>,
    },
    /// `gi`: the field typed in last.
    InputLast {
        count: Option<u32>,
    },
    /// `]i` and `<C-j>`.
    InputNext {
        count: Option<u32>,
    },
    /// `[i` and `<C-k>`.
    InputPrev {
        count: Option<u32>,
    },
    Captcha {
        op: CaptchaOp,
    },
}

impl PageAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::VisualEnter => "visual.enter",
            Self::VisualExit => "visual.exit",
            Self::VisualYank => "visual.yank",
            Self::VisualSwapEnds => "visual.swap_ends",
            Self::VisualMove { motion, .. } => motion_name(*motion),
            Self::VisualObject { .. } => "visual.object",
            Self::Jump { kind } => match kind {
                JumpKind::Hints => "jump.hints",
                JumpKind::Controls => "jump.controls",
                JumpKind::Text => "jump.text",
                JumpKind::Words => "jump.words",
                JumpKind::Land => "jump.land",
                JumpKind::Inputs => "jump.inputs",
                JumpKind::Captcha => "jump.captcha",
            },
            Self::PageHintInput { .. } => "page_hint.input",
            Self::PageHintRefresh => "page_hint.refresh",
            Self::PageHintCancel => "page_hint.cancel",
            Self::InputChoose { .. } => "input.choose",
            Self::InputLast { .. } => "input.last",
            Self::InputNext { .. } => "input.next",
            Self::InputPrev { .. } => "input.prev",
            Self::Captcha { op } => match op {
                CaptchaOp::Verify => "captcha.verify",
                CaptchaOp::Answer => "captcha.answer",
                CaptchaOp::Reload => "captcha.reload",
                CaptchaOp::Audio => "captcha.audio",
                CaptchaOp::Play => "captcha.play",
            },
        }
    }
}

const MOTIONS: [(VisualMotion, &str, &str); 12] = [
    (VisualMotion::Left, "visual.left", "Cursor Left"),
    (VisualMotion::Right, "visual.right", "Cursor Right"),
    (VisualMotion::Down, "visual.down", "Cursor Down"),
    (VisualMotion::Up, "visual.up", "Cursor Up"),
    (VisualMotion::WordForward, "visual.word_forward", "Word Forward"),
    (VisualMotion::WordBackward, "visual.word_backward", "Word Backward"),
    (VisualMotion::LineStart, "visual.line_start", "Line Start"),
    (VisualMotion::LineEnd, "visual.line_end", "Line End"),
    (
        VisualMotion::ParagraphForward,
        "visual.paragraph_forward",
        "Paragraph Forward",
    ),
    (
        VisualMotion::ParagraphBackward,
        "visual.paragraph_backward",
        "Paragraph Backward",
    ),
    (VisualMotion::Top, "visual.top", "Top"),
    (VisualMotion::Bottom, "visual.bottom", "Bottom"),
];

fn motion_name(motion: VisualMotion) -> &'static str {
    MOTIONS
        .iter()
        .find(|(each, _, _)| *each == motion)
        .map(|(_, name, _)| *name)
        .unwrap_or("visual.left")
}

fn find_arg<'a>(args: &'a [(String, String)], key: &str) -> Option<&'a str> {
    args.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn parse_count(name: &str, args: &[(String, String)]) -> Result<Option<u32>, CommandError> {
    find_arg(args, "count")
        .map(|raw| {
            raw.parse::<u32>().map_err(|_| CommandError::InvalidArgument {
                command: name.to_string(),
                message: format!("expected integer `count`, got `{raw}`"),
            })
        })
        .transpose()
}

/// A key's notation (`a`, `A`, `<CR>`, `<BS>`, `<Esc>`, `<Space>`, `<C-l>`) as a jump takes it.
pub fn hint_key_from_notation(raw: &str) -> HintKey {
    match raw {
        "<CR>" | "<Enter>" => HintKey::Enter,
        "<BS>" => HintKey::Backspace,
        "<Esc>" => HintKey::Escape,
        "<Space>" | " " => HintKey::Space,
        _ => {
            let mut chars = raw.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) => HintKey::Char(ch),
                _ if raw.starts_with("<C-") || raw.starts_with("<A-") || raw.starts_with("<D-") => HintKey::Chord,
                _ if raw == "<lt>" => HintKey::Char('<'),
                _ => HintKey::Other,
            }
        }
    }
}

/// Parses the commands of this module, or returns `None` for a name it does not own.
pub fn parse_command(name: &str, args: &[(String, String)]) -> Result<Option<Command>, CommandError> {
    let count = parse_count(name, args)?;
    if let Some((motion, _, _)) = MOTIONS.iter().find(|(_, each, _)| *each == name) {
        return Ok(Some(Command::Page(PageAction::VisualMove { motion: *motion, count })));
    }
    let action = match name {
        "visual.enter" => PageAction::VisualEnter,
        "visual.exit" => PageAction::VisualExit,
        "visual.yank" => PageAction::VisualYank,
        "visual.swap_ends" => PageAction::VisualSwapEnds,
        "visual.object" => PageAction::VisualObject {
            keys: find_arg(args, "keys").unwrap_or("iw").to_string(),
            count,
        },
        "jump.hints" => PageAction::Jump { kind: JumpKind::Hints },
        "jump.controls" => PageAction::Jump {
            kind: JumpKind::Controls,
        },
        "jump.text" => PageAction::Jump { kind: JumpKind::Text },
        "jump.words" => PageAction::Jump { kind: JumpKind::Words },
        "jump.land" => PageAction::Jump { kind: JumpKind::Land },
        "jump.captcha" => PageAction::Jump {
            kind: JumpKind::Captcha,
        },
        "page_hint.input" => PageAction::PageHintInput {
            key: hint_key_from_notation(find_arg(args, "key").unwrap_or_default()),
        },
        "page_hint.refresh" => PageAction::PageHintRefresh,
        "page_hint.cancel" => PageAction::PageHintCancel,
        "input.choose" => PageAction::InputChoose { count },
        "input.last" => PageAction::InputLast { count },
        "input.next" => PageAction::InputNext { count },
        "input.prev" => PageAction::InputPrev { count },
        "captcha.verify" => PageAction::Captcha { op: CaptchaOp::Verify },
        "captcha.answer" => PageAction::Captcha { op: CaptchaOp::Answer },
        "captcha.reload" => PageAction::Captcha { op: CaptchaOp::Reload },
        "captcha.audio" => PageAction::Captcha { op: CaptchaOp::Audio },
        "captcha.play" => PageAction::Captcha { op: CaptchaOp::Play },
        _ => return Ok(None),
    };
    Ok(Some(Command::Page(action)))
}

/// The descriptors of this module's commands.
pub fn descriptors() -> Vec<CommandDescriptor> {
    let mut items: Vec<(&str, &str, &str)> = vec![
        (
            "visual.enter",
            "Visual Mode",
            "Select the page's text from a label or the cursor",
        ),
        (
            "visual.exit",
            "Leave Visual Mode",
            "Leave visual mode; the cursor stays where the selection ended",
        ),
        (
            "visual.yank",
            "Yank Selection",
            "Copy the selection into the clipboard and the yank ring",
        ),
        (
            "visual.swap_ends",
            "Other End of Selection",
            "Move the cursor to the other end of the selection",
        ),
        (
            "visual.object",
            "Select Text Object",
            "Select around or inside a text object",
        ),
        (
            "jump.hints",
            "Label Every Link, Button and Field",
            "Label every control of the page",
        ),
        (
            "jump.controls",
            "Jump to a Link, Button or Field by its Text",
            "Type some of a control's text, then its label",
        ),
        (
            "jump.text",
            "Jump to a Word by its Letters",
            "Type some of the page's text, then its label",
        ),
        (
            "jump.words",
            "Label Every Word and Paragraph",
            "Grow the selection to a word or paragraph",
        ),
        (
            "jump.land",
            "Choose Where Visual Mode Starts",
            "Label every word and paragraph to start visual mode at",
        ),
        (
            "jump.captcha",
            "Label the CAPTCHA to Solve it with the Keys",
            "Label the controls of the page's CAPTCHA",
        ),
        (
            "input.choose",
            "Insert in a Field, by Label if There are Several",
            "Focus a field of the page",
        ),
        (
            "input.last",
            "Insert in Last Used Field",
            "Focus the field typed in last",
        ),
        ("input.next", "Next Field", "Focus the next field of the page"),
        ("input.prev", "Prev Field", "Focus the previous field of the page"),
        (
            "captcha.verify",
            "Verify the CAPTCHA (Verify, Next, Skip)",
            "Press the CAPTCHA's verify button",
        ),
        (
            "captcha.answer",
            "Type the CAPTCHA's Answer",
            "Focus the CAPTCHA's answer field",
        ),
        (
            "captcha.reload",
            "Get a New CAPTCHA Challenge",
            "Ask the CAPTCHA for a new challenge",
        ),
        (
            "captcha.audio",
            "Switch the CAPTCHA to Audio, or Back to Images",
            "Switch the CAPTCHA's challenge",
        ),
        (
            "captcha.play",
            "Play the CAPTCHA's Audio",
            "Play the CAPTCHA's audio challenge",
        ),
    ];
    for (_, name, title) in MOTIONS {
        items.push((name, title, "Move the end of the visual selection"));
    }
    items
        .into_iter()
        .map(|(name, title, description)| CommandDescriptor {
            name: name.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            arguments: Vec::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(id: &str, x: f32, y: f32, distance: f32, nexts: &[&str]) -> PageTarget {
        PageTarget {
            id: id.to_string(),
            x,
            y,
            width: 40.0,
            height: 16.0,
            kind: "control".to_string(),
            nexts: nexts.iter().map(|next| next.to_string()).collect(),
            distance,
            href: None,
        }
    }

    #[test]
    fn hints_are_labeled_in_reading_order() {
        let mut session = PageHintSession::new(TabId(1), JumpKind::Hints, false);
        session.set_targets(vec![
            target("low", 10.0, 200.0, 1.0, &[]),
            target("right", 300.0, 20.0, 2.0, &[]),
            target("left", 10.0, 22.0, 3.0, &[]),
        ]);
        let order: Vec<&str> = session.items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(order, vec!["left", "right", "low"]);
        assert_eq!(session.items[0].label.as_deref(), Some("a"));
        assert_eq!(
            session.feed(HintKey::Char('S')),
            HintOutcome::Pick {
                id: "right".to_string(),
                new_tab: true,
            }
        );
    }

    #[test]
    fn a_key_that_starts_no_label_is_ignored() {
        let mut session = PageHintSession::new(TabId(1), JumpKind::Hints, false);
        session.set_targets(
            (0..30)
                .map(|i| target(&i.to_string(), i as f32, 0.0, 0.0, &[]))
                .collect(),
        );
        assert_eq!(session.feed(HintKey::Char('a')), HintOutcome::Changed);
        assert_eq!(session.typed, "a");
        assert_eq!(
            session.feed(HintKey::Char('k')),
            HintOutcome::Ignored,
            "k is no label letter"
        );
        assert_eq!(session.feed(HintKey::Backspace), HintOutcome::Changed);
        assert_eq!(session.feed(HintKey::Backspace), HintOutcome::Finished);
    }

    #[test]
    fn text_jumps_label_after_text_and_keep_labels() {
        let mut session = PageHintSession::new(TabId(1), JumpKind::Controls, false);
        session.set_targets(vec![target("a", 0.0, 0.0, 5.0, &[]), target("b", 0.0, 50.0, 1.0, &[])]);
        assert!(
            session.items.iter().all(|item| item.label.is_none()),
            "no labels before any text"
        );
        assert_eq!(
            session.feed(HintKey::Char('d')),
            HintOutcome::Ask {
                pattern: "d".to_string()
            }
        );
        assert_eq!(
            session.feed(HintKey::Char('o')),
            HintOutcome::Ignored,
            "waits for the page"
        );
        assert!(session.answer(
            "d",
            vec![target("a", 0.0, 0.0, 5.0, &["o"]), target("b", 0.0, 50.0, 1.0, &["a"])]
        ));
        // Nearest first, and never `a` or `o`, which follow the matches.
        assert_eq!(session.items[0].id, "b");
        assert_eq!(session.items[0].label.as_deref(), Some("s"));
        assert_eq!(session.items[1].label.as_deref(), Some("d"));
        assert_eq!(session.queued.pop_front(), Some(HintKey::Char('o')));

        assert_eq!(
            session.feed(HintKey::Char('o')),
            HintOutcome::Ask {
                pattern: "do".to_string()
            }
        );
        assert!(session.answer("do", vec![target("a", 0.0, 0.0, 5.0, &["c"])]));
        assert_eq!(session.items[0].label.as_deref(), Some("d"), "a keeps its label");
        assert_eq!(
            session.feed(HintKey::Char('D')),
            HintOutcome::Pick {
                id: "a".to_string(),
                new_tab: true,
            }
        );
    }

    #[test]
    fn text_that_matches_nothing_is_not_taken() {
        let mut session = PageHintSession::new(TabId(1), JumpKind::Text, false);
        session.feed(HintKey::Char('z'));
        assert!(!session.answer("z", Vec::new()));
        assert_eq!(session.pattern, "");
        assert_eq!(session.feed(HintKey::Backspace), HintOutcome::Finished);
    }

    #[test]
    fn labels_skip_what_follows_matches() {
        let labels = assign_labels(3, &['A', 's'], &[None, Some("k".to_string()), None]);
        assert_eq!(
            labels,
            vec![Some("d".to_string()), Some("k".to_string()), Some("f".to_string())]
        );
    }

    #[test]
    fn notation_to_hint_keys() {
        assert_eq!(hint_key_from_notation("A"), HintKey::Char('A'));
        assert_eq!(hint_key_from_notation("<CR>"), HintKey::Enter);
        assert_eq!(hint_key_from_notation("<C-l>"), HintKey::Chord);
        assert_eq!(hint_key_from_notation("<Tab>"), HintKey::Other);
    }

    #[test]
    fn all_page_action_descriptors_round_trip() {
        for desc in descriptors() {
            let parsed = Command::from_name_and_args(&desc.name, &[]).unwrap();
            assert_eq!(parsed.name(), desc.name);
        }
    }
}
