/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! CAPTCHA recognition rules, control purpose classification, pure label/cursor geometry math, and the keyboard cursor
//! (`PointerPad`) state machine, ported from `ZendiaCaptcha.sys.mjs` and `ZendiaPointerContent.sys.mjs` (`CAP-01`..`CAP-09`).

use crate::focus::HINT_ALPHABET;
use crate::model::TabId;
use crate::tab_source::{PointerEventKind, PointerStep, TouchStage};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;
use zendia_automation::{CaptchaWatchDetection, CaptchaWatchStateReading, ViewportRect};

/// All check kinds recognized by `ZendiaCaptcha.KINDS`.
pub const KINDS: &[&str] = &[
    "text",
    "image",
    "audio",
    "math",
    "word",
    "video",
    "checkbox",
    "invisible",
    "score",
    "slider",
    "puzzle",
    "rotate",
    "click",
    "icon",
    "game",
    "behavioral",
    "sensor",
    "biometric",
    "proof-of-work",
    "managed",
    "risk",
    "hybrid",
    "honeypot",
    "timing",
];

/// Control purposes inside a CAPTCHA challenge, in priority order (`ZendiaCaptcha.PURPOSES`).
pub const PURPOSES: &[&str] = &["answer", "play", "visual", "audio", "reload", "verify"];

/// Live CAPTCHA states ordered from most user-demanding to least (`ZendiaCaptcha.STATES`).
pub const STATES: &[&str] = &["open", "unchecked", "failed", "shown", "checking", "solved", "none"];

/// Letters used by the keyboard cursor (`PAD_LETTERS`: `HINT_ALPHABET` minus `h`, `j`, `k`, `l`, `v` = 21 letters).
pub const PAD_LETTERS: &str = "asdfgqweruiopzxcbnmty";

/// Staying CAPTCHA jump prompt (`ZendiaJumpContent.sys.mjs:72-73`, `CAP-09`).
pub const STAYING_PROMPT: &str = "captcha · label picks · Label holds · Enter verify · Tab answer · ^R new · ^A audio · ^P play · . cursor · , drag · Esc leave";

/// Empty-page cursor fallback banner (`ZendiaJumpContent.sys.mjs:1791`, `CAP-09`).
pub const NO_CAPTCHA_CURSOR_BANNER: &str = "no CAPTCHA found: the cursor is on the page";

/// Delay after a challenge click before re-reading (`350 ms`, `CHALLENGE_SETTLE_MS`).
pub const CHALLENGE_SETTLE_MS: u64 = 350;
/// Periodic poll interval while staying in a challenge (`900 ms`, `CHALLENGE_POLL_MS`).
pub const CHALLENGE_POLL_MS: u64 = 900;
/// Consecutive empty reads before a staying jump or cursor ends (`3`, `VACANT_POLLS` / `GONE_LOOKS`).
pub const VACANT_POLLS: u8 = 3;
/// Delay after a cursor click before checking whether the CAPTCHA is gone (`700 ms`, `WATCH_AFTER_CLICK_MS`).
pub const WATCH_AFTER_CLICK_MS: u64 = 700;
/// Duration of a `"no <purpose> here"` flash notice (`1500 ms`, `NOTICE_MS`).
pub const STAYING_NOTICE_MS: u64 = 1500;
/// Duration of a `"click"` flash notice in the cursor prompt (`600 ms`).
pub const CURSOR_NOTICE_MS: u64 = 600;
/// How long a gone subframe holding a CAPTCHA state is kept while waiting for its replacement (`ZendiaCaptchaWatch.REPLACE_WAIT_MS`).
pub const REPLACE_WAIT_MS: u64 = 2000;

const MAX_DETECTIONS: usize = 50;
const MAX_URL: usize = 2048;
const MAX_TEXT: usize = 256;
const CELL_SIZE: f32 = 40.0;
const FINE_CELL_SIZE: f32 = 22.0;
const MAX_CELLS: usize = 21 * 21;
const STEP: f32 = 2.0;
const FAR_STEP: f32 = 10.0;
const FINE_STEP: f32 = 1.0;
const RULER_HEIGHT: f32 = 0.7;
const RULER_LABEL_ROOM: f32 = 30.0;
const PATH_STEP: f32 = 12.0;
const MOVE_PAUSE_MS: u64 = 8;

/// Human-readable name for a missing CAPTCHA purpose (`PURPOSE_NAMES`).
pub fn purpose_display_name(purpose: &str) -> &'static str {
    match purpose {
        "verify" => "verify button",
        "answer" => "answer field",
        "reload" => "new-challenge button",
        "audio" => "audio challenge",
        "visual" => "image challenge",
        "play" => "play button",
        _ => "button",
    }
}

struct CaptchaServiceRule {
    name: &'static str,
    frame: Regex,
    challenge: Option<Regex>,
    kind: &'static str,
    widget_kind: Option<&'static str>,
    has_recaptcha_score: bool,
}

struct ScriptRule {
    name: &'static str,
    script: Regex,
    kind: &'static str,
    requires_recaptcha_score: bool,
}

static SERVICE_SITES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^https?://www\.(?:hcaptcha|arkoselabs|geetest|mtcaptcha|friendlycaptcha|leminnow|keycaptcha)\.(?:com|cn|eu)/",
    )
    .unwrap()
});

static RENDER_PARAM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[?&]render=([^&#]*)").unwrap());
static EXPLICIT_OR_ONLOAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?:explicit|onload)\b").unwrap());

fn has_scored_render_param(url: &str) -> bool {
    RENDER_PARAM.captures_iter(url).any(|cap| {
        let val = cap.get(1).map_or("", |m| m.as_str());
        !EXPLICIT_OR_ONLOAD.is_match(val)
    })
}

static CAPTCHAS: LazyLock<Vec<CaptchaServiceRule>> = LazyLock::new(|| {
    vec![
        CaptchaServiceRule {
            name: "reCAPTCHA",
            frame: Regex::new(r"google\.com/recaptcha/|recaptcha\.net/").unwrap(),
            challenge: Some(Regex::new(r"/bframe").unwrap()),
            kind: "image",
            widget_kind: Some("checkbox"),
            has_recaptcha_score: true,
        },
        CaptchaServiceRule {
            name: "hCaptcha",
            frame: Regex::new(r"hcaptcha\.com/").unwrap(),
            challenge: Some(Regex::new(r"frame=challenge").unwrap()),
            kind: "image",
            widget_kind: Some("checkbox"),
            has_recaptcha_score: false,
        },
        CaptchaServiceRule {
            name: "Cloudflare Turnstile",
            frame: Regex::new(r"challenges\.cloudflare\.com/|/cdn-cgi/challenge-platform/").unwrap(),
            challenge: None,
            kind: "managed",
            widget_kind: None,
            has_recaptcha_score: false,
        },
        CaptchaServiceRule {
            name: "Arkose",
            frame: Regex::new(r"arkoselabs\.com/|funcaptcha\.com/|octocaptcha\.com/").unwrap(),
            challenge: Some(Regex::new(r".").unwrap()),
            kind: "game",
            widget_kind: None,
            has_recaptcha_score: false,
        },
        CaptchaServiceRule {
            name: "GeeTest",
            frame: Regex::new(r"geetest\.com/|geevisit\.com/|geetest\.cn/").unwrap(),
            challenge: Some(Regex::new(r".").unwrap()),
            kind: "slider",
            widget_kind: None,
            has_recaptcha_score: false,
        },
        CaptchaServiceRule {
            name: "HUMAN",
            frame: Regex::new(r"px-cdn\.net/|px-cloud\.net/|perimeterx\.(?:net|com)/|human\.com/captcha").unwrap(),
            challenge: Some(Regex::new(r".").unwrap()),
            kind: "behavioral",
            widget_kind: None,
            has_recaptcha_score: false,
        },
        CaptchaServiceRule {
            name: "DataDome",
            frame: Regex::new(r"captcha-delivery\.com/").unwrap(),
            challenge: Some(Regex::new(r".").unwrap()),
            kind: "slider",
            widget_kind: None,
            has_recaptcha_score: false,
        },
        CaptchaServiceRule {
            name: "Yandex SmartCaptcha",
            frame: Regex::new(r"smartcaptcha\.yandexcloud\.net/|captcha\.yandex\.").unwrap(),
            challenge: Some(Regex::new(r"advanced").unwrap()),
            kind: "puzzle",
            widget_kind: Some("checkbox"),
            has_recaptcha_score: false,
        },
        CaptchaServiceRule {
            name: "Tencent Captcha",
            frame: Regex::new(r"captcha\.qcloud\.com/|turing\.captcha\.gtimg\.com/").unwrap(),
            challenge: Some(Regex::new(r".").unwrap()),
            kind: "slider",
            widget_kind: None,
            has_recaptcha_score: false,
        },
        CaptchaServiceRule {
            name: "NetEase Yidun",
            frame: Regex::new(r"(?:c|captcha)\.dun\.163\.com/").unwrap(),
            challenge: Some(Regex::new(r".").unwrap()),
            kind: "slider",
            widget_kind: None,
            has_recaptcha_score: false,
        },
        CaptchaServiceRule {
            name: "MTCaptcha",
            frame: Regex::new(r"mtcaptcha\.com/").unwrap(),
            challenge: None,
            kind: "text",
            widget_kind: None,
            has_recaptcha_score: false,
        },
        CaptchaServiceRule {
            name: "Friendly Captcha",
            frame: Regex::new(r"\bfrcapi\.com/|\b(?:eu-)?api\.friendlycaptcha\.(?:com|eu)/").unwrap(),
            challenge: None,
            kind: "proof-of-work",
            widget_kind: None,
            has_recaptcha_score: false,
        },
        CaptchaServiceRule {
            name: "AWS WAF",
            frame: Regex::new(r"awswaf\.com/").unwrap(),
            challenge: Some(Regex::new(r".").unwrap()),
            kind: "puzzle",
            widget_kind: None,
            has_recaptcha_score: false,
        },
        CaptchaServiceRule {
            name: "Lemin",
            frame: Regex::new(r"leminnow\.com/").unwrap(),
            challenge: Some(Regex::new(r".").unwrap()),
            kind: "puzzle",
            widget_kind: None,
            has_recaptcha_score: false,
        },
        CaptchaServiceRule {
            name: "KeyCAPTCHA",
            frame: Regex::new(r"keycaptcha\.com/").unwrap(),
            challenge: Some(Regex::new(r".").unwrap()),
            kind: "puzzle",
            widget_kind: None,
            has_recaptcha_score: false,
        },
    ]
});

static SCRIPTS: LazyLock<Vec<ScriptRule>> = LazyLock::new(|| {
    vec![
        ScriptRule {
            name: "reCAPTCHA",
            script: Regex::new(r"recaptcha/[^?]*\.js\?[^#]*\brender=").unwrap(),
            kind: "score",
            requires_recaptcha_score: true,
        },
        ScriptRule {
            name: "reCAPTCHA",
            script: Regex::new(r"google\.com/recaptcha/|recaptcha\.net/|gstatic\.com/recaptcha/").unwrap(),
            kind: "checkbox",
            requires_recaptcha_score: false,
        },
        ScriptRule {
            name: "hCaptcha",
            script: Regex::new(r"(?:js\.)?hcaptcha\.com/1/api\.js").unwrap(),
            kind: "checkbox",
            requires_recaptcha_score: false,
        },
        ScriptRule {
            name: "Cloudflare Turnstile",
            script: Regex::new(r"challenges\.cloudflare\.com/turnstile/").unwrap(),
            kind: "managed",
            requires_recaptcha_score: false,
        },
        ScriptRule {
            name: "Arkose",
            script: Regex::new(r"arkoselabs\.com/|funcaptcha\.com/").unwrap(),
            kind: "game",
            requires_recaptcha_score: false,
        },
        ScriptRule {
            name: "GeeTest",
            script: Regex::new(r"geetest\.(?:com|cn)/|geevisit\.com/|/gt4[\w.-]*\.js(?:[?#]|$)").unwrap(),
            kind: "slider",
            requires_recaptcha_score: false,
        },
        ScriptRule {
            name: "DataDome",
            script: Regex::new(r"js\.datadome\.co/tags\.js").unwrap(),
            kind: "behavioral",
            requires_recaptcha_score: false,
        },
        ScriptRule {
            name: "HUMAN",
            script: Regex::new(r"px-cdn\.net/|px-cloud\.net/").unwrap(),
            kind: "behavioral",
            requires_recaptcha_score: false,
        },
        ScriptRule {
            name: "AWS WAF",
            script: Regex::new(r"awswaf\.com/").unwrap(),
            kind: "puzzle",
            requires_recaptcha_score: false,
        },
        ScriptRule {
            name: "Friendly Captcha",
            script: Regex::new(
                r"@friendlycaptcha/sdk|friendly-challenge|\bfrcapi\.com/|\b(?:eu-)?api\.friendlycaptcha\.(?:com|eu)/",
            )
            .unwrap(),
            kind: "proof-of-work",
            requires_recaptcha_score: false,
        },
        ScriptRule {
            name: "Lemin",
            script: Regex::new(r"leminnow\.com/").unwrap(),
            kind: "puzzle",
            requires_recaptcha_score: false,
        },
        ScriptRule {
            name: "ALTCHA",
            script: Regex::new(r"/altcha[@/.]").unwrap(),
            kind: "proof-of-work",
            requires_recaptcha_score: false,
        },
        ScriptRule {
            name: "MTCaptcha",
            script: Regex::new(r"mtcaptcha\.com/").unwrap(),
            kind: "text",
            requires_recaptcha_score: false,
        },
        ScriptRule {
            name: "Yandex SmartCaptcha",
            script: Regex::new(r"smartcaptcha\.yandexcloud\.net/|captcha\.yandex\.").unwrap(),
            kind: "puzzle",
            requires_recaptcha_score: false,
        },
        ScriptRule {
            name: "Kasada",
            script: Regex::new(r"/ips\.js(?:[?#]|$)").unwrap(),
            kind: "sensor",
            requires_recaptcha_score: false,
        },
    ]
});

const TOKEN_FIELDS: &[&str] = &[
    "g-recaptcha-response",
    "h-captcha-response",
    "cf-turnstile-response",
    "frc-captcha-solution",
    "frc-captcha-response",
    "mtcaptcha-verifiedtoken",
    "aws-waf-token",
    "fc-token",
    "altcha",
    "geetest_challenge",
    "geetest_validate",
    "geetest_seccode",
    "lemin_answer",
];

static TOKEN_PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\.([A-Z][A-Z_]*)$").unwrap());
static PLACEHOLDER_FAILED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|_)(?:EXPIRED|ERROR|TIMEOUT|RESET|DESTROYED|FAILED)$").unwrap());
static PLACEHOLDER_RUNNING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:FETCHING|UNFINISHED|SOLVING|REQUESTING|VERIFYING|ACTIVATED)$").unwrap());
static NOT_A_TOKEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^(?:null|undefined|false|0)$").unwrap());
static HONEYPOT_NAMES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:hp|honeypot|honey|sweet|website|homepage|url|link|email2|e-?mail2|fax|company|leave_?blank|do_?not_?fill|keep_?empty|comment_?hp)$")
        .unwrap()
});
static OWN_CAPTCHA_NAMES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)captcha|security[\s_-]?code|verification[\s_-]?code|verify[\s_-]?code|anti[\s_-]?bot|human[\s_-]?check|enter (?:the )?(?:characters|code|text|letters|numbers) (?:you see|shown|above|below|in the (?:image|picture))")
        .unwrap()
});
static NOT_OWN_CAPTCHA: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)logo|icon|badge|recaptcha-response|captcha-response").unwrap());
static NOT_A_PURPOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)download|help|privacy|terms|report|feedback|logo").unwrap());
static PURPOSE_RULES: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    vec![
        (
            "play",
            Regex::new(r"(?i)\bplay\b|\breplay\b|listen again|play-?button").unwrap(),
        ),
        (
            "visual",
            Regex::new(r"(?i)\bvisual\b|image-?button|get an? (?:image|picture)|(?:image|picture) challenge instead")
                .unwrap(),
        ),
        (
            "audio",
            Regex::new(r"(?i)\baudio\b|\blisten\b|headphone|\bsound\b|\bvoice\b").unwrap(),
        ),
        (
            "reload",
            Regex::new(r"(?i)reload|refresh|new challenge|another challenge|try another|try again|start over|restart|^reset\b|new (?:image|picture|puzzle|question)s?\b")
                .unwrap(),
        ),
        (
            "verify",
            Regex::new(r"(?i)\bverify\b|\bsubmit\b|^next\b|\bskip\b|^check\b|\bconfirm\b|^done\b|^continue\b|^ok\b|verify-?button|button-?submit")
                .unwrap(),
        ),
    ]
});
static SOLVED_CLASS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:^|[_-])(?:success|verified|solved|passed)$").unwrap());
static CHALLENGE_PROMPTS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    vec![
        (
            "image",
            Regex::new(r"(?i)^(?:please )?(?:select|click|tap|pick)(?: on)?(?: all| each| every| any)?(?: of)?(?: the)? (?:images?|squares?|pictures?|tiles?|photos?|boxes)\b")
                .unwrap(),
        ),
        (
            "text",
            Regex::new(r"(?i)^(?:please )?(?:type|enter)(?: in)?(?: the)? (?:characters?|letters?|text|code|words?|numbers?)\b.*\b(?:see|shown|image|picture|above|below)\b")
                .unwrap(),
        ),
        (
            "slider",
            Regex::new(r"(?i)^(?:please )?(?:slide|drag)\b.*\b(?:into place|gap|to (?:complete|fit|verify)|right)\b")
                .unwrap(),
        ),
        (
            "rotate",
            Regex::new(r"(?i)^(?:please )?(?:rotate|turn) the (?:image|picture|object)\b").unwrap(),
        ),
        (
            "click",
            Regex::new(r"(?i)^(?:please )?(?:click|tap|select|pick)(?: on)?(?: the)?(?: (?:icons?|characters?|symbols?|letters?|objects?|images?))? (?:in|with) (?:the |this )?(?:following |given )?order\b")
                .unwrap(),
        ),
        (
            "math",
            Regex::new(r"(?i)^(?:what is|solve|calculate|how much is) \d+ ?[-+x×*÷/] ?\d+").unwrap(),
        ),
        ("behavioral", Regex::new(r"(?i)^press (?:&|and) hold\b").unwrap()),
        (
            "checkbox",
            Regex::new(r"(?i)^(?:i['’]?m|i am) (?:not a robot|a human|human)\.?$|^verify (?:that )?you(?:['’]re| are)(?: a)? human\.?$|^(?:are you a )?robot or human\??$|^click (?:here )?to verify\.?$")
                .unwrap(),
        ),
    ]
});

fn service_of(url: &str) -> Option<&'static CaptchaServiceRule> {
    if SERVICE_SITES.is_match(url) {
        return None;
    }
    CAPTCHAS.iter().find(|rule| rule.frame.is_match(url))
}

fn loader_of(url: &str) -> Option<&'static ScriptRule> {
    if SERVICE_SITES.is_match(url) {
        return None;
    }
    SCRIPTS
        .iter()
        .find(|rule| rule.script.is_match(url) && (!rule.requires_recaptcha_score || has_scored_render_param(url)))
}

/// Returns whichever CAPTCHA state demands more of the user (`ZendiaCaptcha.stronger`).
pub fn stronger<'a>(a: &'a str, b: &'a str) -> &'a str {
    let rank = |s: &str| STATES.iter().position(|&each| each == s).unwrap_or(STATES.len());
    if rank(b) < rank(a) { b } else { a }
}

/// Whether `state` requires user action (`open`, `unchecked`, `failed`, `shown`).
pub fn asks_user(state: &str) -> bool {
    matches!(state, "open" | "unchecked" | "failed" | "shown")
}

/// Whether `state` holds the page up (`asks_user` or `checking`).
pub fn holds_up(state: &str) -> bool {
    asks_user(state) || state == "checking"
}

/// Whether `state` is passed or absent (`solved` or `none`).
pub fn is_cleared(state: &str) -> bool {
    matches!(state, "solved" | "none")
}

/// Whether a widget's class list or `data-state` marks it as solved (`ZendiaCaptcha.marksSolved`).
pub fn marks_solved<'a>(classes: impl IntoIterator<Item = &'a str>, state: &str) -> bool {
    state == "verified" || classes.into_iter().any(|c| SOLVED_CLASS.is_match(c))
}

/// Service name of a CAPTCHA frame URL, if any (`ZendiaCaptcha.nameOf`).
pub fn name_of(url: &str) -> Option<&'static str> {
    service_of(url).map(|r| r.name)
}

/// Whether a frame URL is the interactive challenge part of a CAPTCHA (`ZendiaCaptcha.isChallenge`).
pub fn is_challenge(url: &str) -> bool {
    service_of(url)
        .and_then(|r| r.challenge.as_ref())
        .is_some_and(|re| re.is_match(url))
}

/// Kind of check for a frame or loader script URL (`ZendiaCaptcha.kindOf`).
pub fn kind_of(url: &str) -> Option<&'static str> {
    let Some(captcha) = service_of(url) else {
        return kind_of_script(url);
    };
    if captcha.has_recaptcha_score && has_scored_render_param(url) {
        return Some("score");
    }
    let Some(challenge) = captcha.challenge.as_ref() else {
        return Some(captcha.kind);
    };
    if challenge.is_match(url) {
        Some(captcha.kind)
    } else {
        Some(captcha.widget_kind.unwrap_or(captcha.kind))
    }
}

/// Service name of a loader script URL (`ZendiaCaptcha.nameOfScript`).
pub fn name_of_script(url: &str) -> Option<&'static str> {
    loader_of(url).map(|r| r.name)
}

/// Kind of check armed by a loader script URL (`ZendiaCaptcha.kindOfScript`).
pub fn kind_of_script(url: &str) -> Option<&'static str> {
    loader_of(url).map(|r| r.kind)
}

/// Kind of check asked by a short page prompt (`ZendiaCaptcha.promptKind`).
pub fn prompt_kind(text: &str) -> Option<&'static str> {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() || collapsed.chars().count() > 120 {
        return None;
    }
    CHALLENGE_PROMPTS
        .iter()
        .find(|(_, rule)| rule.is_match(&collapsed))
        .map(|(kind, _)| *kind)
}

/// Whether a form field name is a CAPTCHA answer token field (`ZendiaCaptcha.isTokenField`).
pub fn is_token_field(name: &str) -> bool {
    let field = name.to_ascii_lowercase();
    TOKEN_FIELDS.iter().any(|token| {
        field == *token || field.starts_with(&format!("{token}-")) || field.starts_with(&format!("{token}["))
    })
}

/// State implied by a CAPTCHA answer field's value (`ZendiaCaptcha.tokenState`).
pub fn token_state(value: &str) -> Option<&'static str> {
    let token = value.trim();
    if token.is_empty() || NOT_A_TOKEN.is_match(token) {
        return None;
    }
    let Some(cap) = TOKEN_PLACEHOLDER.captures(token) else {
        return Some("solved");
    };
    let placeholder = cap.get(1).map_or("", |m| m.as_str());
    if PLACEHOLDER_FAILED.is_match(placeholder) {
        return Some("failed");
    }
    if PLACEHOLDER_RUNNING.is_match(placeholder) {
        Some("checking")
    } else {
        None
    }
}

/// Whether a form field is a hidden honeypot trap (`ZendiaCaptcha.isHoneypotField`).
pub fn is_honeypot_field(name: &str, visible: bool) -> bool {
    !visible && HONEYPOT_NAMES.is_match(name)
}

/// Whether attribute names/labels identify a site's own CAPTCHA (`ZendiaCaptcha.namesOwnCaptcha`).
pub fn names_own_captcha(names: &str) -> bool {
    let slice: String = names.chars().take(1000).collect();
    OWN_CAPTCHA_NAMES.is_match(&slice) && !NOT_OWN_CAPTCHA.is_match(&slice)
}

/// Input descriptor for [`purpose_of`].
#[derive(Clone, Debug, Default)]
pub struct ControlInfo<'a> {
    pub tag: &'a str,
    pub r#type: &'a str,
    pub editable: bool,
    pub label: &'a str,
    pub name: &'a str,
}

/// Classifies what a control inside a CAPTCHA does (`ZendiaCaptcha.purposeOf`).
pub fn purpose_of(control: ControlInfo<'_>) -> Option<&'static str> {
    let tag = control.tag.to_ascii_lowercase();
    let ty = control.r#type.to_ascii_lowercase();
    let is_answer_type = matches!(ty.as_str(), "" | "text" | "search" | "tel" | "number" | "email" | "url");
    if control.editable && tag != "select" && (tag != "input" || is_answer_type) {
        return Some("answer");
    }
    if tag == "input" && matches!(ty.as_str(), "checkbox" | "radio") {
        return None;
    }
    let label: String = control
        .label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(200)
        .collect();
    let name: String = control
        .name
        .chars()
        .map(|c| if c == '_' || c.is_whitespace() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(200)
        .collect();
    if NOT_A_PURPOSE.is_match(&label) || NOT_A_PURPOSE.is_match(&name) {
        return None;
    }
    for text in [&label, &name] {
        if text.is_empty() {
            continue;
        }
        for (purpose, rule) in PURPOSE_RULES.iter() {
            if rule.is_match(text) {
                return Some(*purpose);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------------------------------
// Pure geometry & label math (`ZendiaCaptcha.sys.mjs:756-989`)
// ---------------------------------------------------------------------------------------------------------------------

/// Assigns sticky equal-length labels so targets keep their previous labels while they remain on screen (`stickyLabels`).
pub fn sticky_labels(keys: &[String], kept: &BTreeMap<String, String>, alphabet: &str) -> Vec<Option<String>> {
    let letters: Vec<char> = alphabet.chars().collect();
    if letters.is_empty() || keys.is_empty() {
        return Vec::new();
    }
    let known: Vec<Option<String>> = keys.iter().map(|k| kept.get(k).cloned()).collect();
    let lengths: BTreeSet<usize> = known.iter().flatten().map(|l| l.chars().count()).collect();
    let length = if lengths.len() == 1 {
        lengths.into_iter().next()
    } else {
        None
    };
    let reuse = length.is_some();
    let size = length.unwrap_or(if keys.len() <= letters.len() { 1 } else { 2 });
    let mut labels: Vec<Option<String>> = vec![None; keys.len()];
    let mut used: BTreeSet<String> = BTreeSet::new();
    if reuse {
        for (i, maybe_label) in known.into_iter().enumerate() {
            if let Some(label) = maybe_label
                && !used.contains(&label)
            {
                used.insert(label.clone());
                labels[i] = Some(label);
            }
        }
    }
    let mut candidates: Vec<String> = Vec::new();
    if size == 1 {
        candidates.extend(letters.iter().map(|c| c.to_string()));
    } else {
        for &first in &letters {
            for &second in &letters {
                candidates.push(format!("{first}{second}"));
            }
        }
    }
    let mut free = candidates.into_iter();
    for slot in &mut labels {
        if slot.is_some() {
            continue;
        }
        let next = free.by_ref().find(|cand| !used.contains(cand));
        let Some(val) = next else {
            break;
        };
        used.insert(val.clone());
        *slot = Some(val);
    }
    labels
}

/// Equal-length prefix-free labels for `count` items from `letters` (`ZendiaCaptcha.labelsOf`).
pub fn labels_of(count: usize, letters: &str) -> Vec<String> {
    if count == 0 || letters.is_empty() {
        return Vec::new();
    }
    let chars: Vec<char> = letters.chars().collect();
    let mut labels: Vec<String> = chars.iter().map(|c| c.to_string()).collect();
    while labels.len() < count {
        labels = labels
            .into_iter()
            .flat_map(|first| chars.iter().map(move |next| format!("{first}{next}")))
            .collect();
    }
    labels.truncate(count);
    labels
}

/// Evenly spaced horizontal ruler stops along the remaining travel of a slider handle (`ZendiaCaptcha.rulerStops`).
pub fn ruler_stops(
    piece_x: f32,
    handle_left: f32,
    handle_width: f32,
    area_left: f32,
    area_width: f32,
    count: usize,
) -> Vec<f32> {
    let room = area_left + area_width - (handle_left + handle_width);
    if count < 1 || !room.is_finite() || room < count as f32 {
        return Vec::new();
    }
    let step = room / count as f32;
    (0..count).map(|i| piece_x + step * (i + 1) as f32).collect()
}

/// Appends `digit` to a vim count string, rejecting leading `0` and clamping at `max` (`ZendiaCaptcha.growCount`).
pub fn grow_count(count: &str, digit: char, max: u32) -> String {
    if !digit.is_ascii_digit() || (count.is_empty() && digit == '0') {
        return count.to_string();
    }
    let grown = format!("{count}{digit}");
    match grown.parse::<u32>() {
        Ok(n) if n > max => max.to_string(),
        Ok(_) => grown,
        Err(_) => max.to_string(),
    }
}

/// Splits `rect` into `columns * rows` equal cells row by row (`ZendiaCaptcha.splitRect`).
pub fn split_rect(rect: ViewportRect, columns: usize, rows: usize) -> Vec<ViewportRect> {
    let columns = columns.max(1);
    let rows = rows.max(1);
    let width = rect.width / columns as f32;
    let height = rect.height / rows as f32;
    let mut parts = Vec::with_capacity(columns * rows);
    for row in 0..rows {
        for column in 0..columns {
            parts.push(ViewportRect {
                x: rect.x + column as f32 * width,
                y: rect.y + row as f32 * height,
                width,
                height,
            });
        }
    }
    parts
}

/// Covers `area` with cells of approximately `cell` CSS pixels (`ZendiaCaptcha.gridCells`).
pub fn grid_cells(area: ViewportRect, cell: f32) -> Vec<ViewportRect> {
    let columns = ((area.width / cell).round() as isize).max(1) as usize;
    let rows = ((area.height / cell).round() as isize).max(1) as usize;
    split_rect(area, columns, rows)
}

/// Center point `(x, y)` of `rect` (`ZendiaCaptcha.middle`).
pub fn middle(rect: ViewportRect) -> (f32, f32) {
    (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

/// Infers a regular board lattice from 3 or more pieces on it (`ZendiaCaptcha.lattice`).
pub fn lattice(pieces: &[ViewportRect]) -> Option<Vec<ViewportRect>> {
    if pieces.len() < 3 {
        return None;
    }
    let axis = |mut values: Vec<f32>| -> Vec<f32> {
        values.sort_by(|a, b| a.total_cmp(b));
        let mut lines: Vec<f32> = Vec::new();
        for value in values {
            if lines.last().is_none_or(|last| value - *last > 4.0) {
                lines.push(value);
            }
        }
        lines
    };
    let centers: Vec<(f32, f32)> = pieces.iter().copied().map(middle).collect();
    let xs = axis(centers.iter().map(|p| p.0).collect());
    let ys = axis(centers.iter().map(|p| p.1).collect());
    let gaps = |lines: &[f32]| -> Vec<f32> { lines.windows(2).map(|w| w[1] - w[0]).collect() };
    let mut pitches = gaps(&xs);
    pitches.extend(gaps(&ys));
    let pitch = pitches.into_iter().reduce(f32::min)?;
    let fits = |lines: &[f32]| -> bool {
        lines.iter().all(|line| {
            let steps = (line - lines[0]) / pitch;
            (steps - steps.round()).abs() < 0.2
        })
    };
    if pitch < 12.0 || !fits(&xs) || !fits(&ys) {
        return None;
    }
    let width = pieces[0].width;
    let height = pieces[0].height;
    let columns = ((xs[xs.len() - 1] - xs[0]) / pitch).round() as usize + 1;
    let rows = ((ys[ys.len() - 1] - ys[0]) / pitch).round() as usize + 1;
    if columns < 2 && rows < 2 {
        return None;
    }
    let mut cells = Vec::with_capacity(columns * rows);
    for row in 0..rows {
        for column in 0..columns {
            cells.push(ViewportRect {
                x: xs[0] + column as f32 * pitch - width / 2.0,
                y: ys[0] + row as f32 * pitch - height / 2.0,
                width,
                height,
            });
        }
    }
    Some(cells)
}

/// Interpolated points from `from` (exclusive) up to `to` (inclusive) with step length at most `step` (`ZendiaCaptcha.path`).
pub fn path(from: (f32, f32), to: (f32, f32), step: f32) -> Vec<(f32, f32)> {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let length = (dx * dx + dy * dy).sqrt();
    let count = ((length / step).ceil() as usize).max(1);
    (0..count)
        .map(|i| {
            let t = (i + 1) as f32 / count as f32;
            (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)
        })
        .collect()
}

/// Whether two rectangles overlap (`ZendiaPointerContent.meets`).
pub fn rects_meet(a: ViewportRect, b: ViewportRect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

// ---------------------------------------------------------------------------------------------------------------------
// Live CAPTCHA Watch Service (`ZendiaCaptchaWatch.sys.mjs`)
// ---------------------------------------------------------------------------------------------------------------------

fn truncate_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

/// Sanitizes raw detections coming from a content frame (`ZendiaCaptchaWatch.sanitize`).
pub fn sanitize_detections(detections: &[CaptchaWatchDetection]) -> Vec<CaptchaWatchDetection> {
    detections
        .iter()
        .take(MAX_DETECTIONS)
        .filter_map(|detection| {
            let signal = truncate_chars(&detection.signal, 32);
            if !matches!(
                signal.as_str(),
                "frame" | "script" | "widget" | "own" | "token" | "honeypot"
            ) {
                return None;
            }
            Some(CaptchaWatchDetection {
                signal,
                name: detection.name.as_deref().map(|s| truncate_chars(s, 64)),
                kind: detection.kind.as_deref().map(|s| truncate_chars(s, 64)),
                challenge: detection.challenge,
                hidden: detection.hidden,
                detail: detection.detail.as_deref().map(|s| truncate_chars(s, MAX_TEXT)),
            })
        })
        .collect()
}

/// Sanitizes a frame's live CAPTCHA state reading (`ZendiaCaptchaWatch.sanitizeState`).
pub fn sanitize_state(reading: &CaptchaWatchStateReading) -> CaptchaWatchStateReading {
    let state = truncate_chars(&reading.state, 16);
    if !STATES.contains(&state.as_str()) {
        return CaptchaWatchStateReading {
            state: "none".to_string(),
            name: None,
            how: None,
        };
    }
    CaptchaWatchStateReading {
        state,
        name: reading.name.as_deref().map(|s| truncate_chars(s, 64)),
        how: reading.how.as_deref().map(|s| truncate_chars(s, 160)),
    }
}

/// One-line human summary of a tab's sanitized detections (`ZendiaCaptchaWatch.describe`).
pub fn describe_detections(detections: &[CaptchaWatchDetection]) -> String {
    // Preserve insertion order of services and kinds, matching JS `Map` and `Set`.
    let mut services: Vec<(String, Vec<String>)> = Vec::new();
    for det in detections {
        if (det.signal == "frame" || det.signal == "script")
            && let Some(name) = det.name.as_deref().filter(|s| !s.is_empty())
        {
            let kinds = if let Some((_, kinds)) = services.iter_mut().find(|(n, _)| n == name) {
                kinds
            } else {
                services.push((name.to_string(), Vec::new()));
                &mut services.last_mut().unwrap().1
            };
            if let Some(kind) = det.kind.as_deref().filter(|s| !s.is_empty())
                && !kinds.iter().any(|k| k == kind)
            {
                kinds.push(kind.to_string());
            }
        }
    }
    let mut parts: Vec<String> = services
        .into_iter()
        .map(|(name, kinds)| {
            if kinds.is_empty() {
                name
            } else {
                format!("{name} ({})", kinds.join(", "))
            }
        })
        .collect();
    if let Some(detail) = detections
        .iter()
        .find(|d| d.signal == "widget")
        .and_then(|d| d.detail.as_deref())
        .filter(|s| !s.is_empty())
    {
        parts.push(format!("{detail} page widget{}", if detail == "1" { "" } else { "s" }));
    }
    for det in detections.iter().filter(|d| d.signal == "own") {
        let kind_part = det
            .kind
            .as_deref()
            .filter(|s| !s.is_empty())
            .map_or_else(String::new, |k| format!("{k} "));
        let detail_part = det
            .detail
            .as_deref()
            .filter(|s| !s.is_empty())
            .map_or_else(String::new, |d| format!(" \"{}\"", truncate_chars(d, 80)));
        parts.push(format!("page-drawn {kind_part}challenge{detail_part}"));
    }
    let details = |signal: &str| -> Vec<&str> {
        detections
            .iter()
            .filter(|d| d.signal == signal)
            .filter_map(|d| d.detail.as_deref().filter(|s| !s.is_empty()))
            .collect()
    };
    let tokens = details("token");
    if !tokens.is_empty() {
        parts.push(format!(
            "token field{} {}",
            if tokens.len() == 1 { "" } else { "s" },
            tokens.join(", ")
        ));
    }
    let traps = details("honeypot");
    if !traps.is_empty() {
        parts.push(format!(
            "honeypot field{} {}",
            if traps.len() == 1 { "" } else { "s" },
            traps.join(", ")
        ));
    }
    parts.join("; ")
}

/// Stable order-independent signature for a list of detections (`ZendiaCaptchaWatch.signatureOf`).
pub fn signature_of_detections(detections: &[CaptchaWatchDetection]) -> String {
    let mut keys: Vec<String> = detections
        .iter()
        .map(|d| {
            format!(
                "{}:{}:{}:{}:{}",
                d.signal,
                d.name.as_deref().unwrap_or(""),
                d.kind.as_deref().unwrap_or(""),
                d.detail.as_deref().unwrap_or(""),
                if d.hidden { "hidden" } else { "" }
            )
        })
        .collect();
    keys.sort();
    keys.join("|")
}

/// Merged live CAPTCHA finding across all frames of a tab (`ZendiaCaptchaWatch.lastFor`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptchaWatchRecord {
    pub url: String,
    pub detections: Vec<CaptchaWatchDetection>,
    pub state: String,
    pub how: Option<String>,
    pub text: String,
    pub signature: String,
    pub at_ms: u64,
}

#[derive(Clone, Debug)]
struct FrameWatchEntry {
    url: String,
    detections: Vec<CaptchaWatchDetection>,
    state: CaptchaWatchStateReading,
    top: bool,
    /// Deadline (`now_ms + REPLACE_WAIT_MS`) while waiting for a replaced subframe to report.
    leaving_until_ms: Option<u64>,
}

/// Per-tab live CAPTCHA watcher merging top and child frame reports (`ZendiaCaptchaWatchService`).
#[derive(Clone, Debug, Default)]
pub struct CaptchaWatchService {
    frames: BTreeMap<TabId, BTreeMap<u64, FrameWatchEntry>>,
    last: BTreeMap<TabId, CaptchaWatchRecord>,
    announced: BTreeMap<TabId, String>,
}

impl CaptchaWatchService {
    pub fn last_for(&self, tab_id: TabId) -> Option<&CaptchaWatchRecord> {
        self.last.get(&tab_id)
    }

    /// Merges a single frame's report into `tab_id` and returns `(merged_record, changed_announcement)`.
    #[allow(clippy::too_many_arguments)]
    pub fn report(
        &mut self,
        tab_id: TabId,
        url: &str,
        detections: &[CaptchaWatchDetection],
        state: &CaptchaWatchStateReading,
        frame_id: u64,
        top: bool,
        now_ms: u64,
    ) -> (Option<CaptchaWatchRecord>, bool) {
        let detections = sanitize_detections(detections);
        let state = sanitize_state(state);
        self.sweep_tab_expired(tab_id, now_ms);
        let frames = self.frames.entry(tab_id).or_default();
        if !top && state.state != "none" {
            frames.retain(|_, entry| entry.leaving_until_ms.is_none());
        }
        if !detections.is_empty() || state.state != "none" {
            frames.insert(
                frame_id,
                FrameWatchEntry {
                    url: truncate_chars(url, MAX_URL),
                    detections,
                    state,
                    top,
                    leaving_until_ms: None,
                },
            );
        } else if frames.remove(&frame_id).is_none() {
            return (self.last.get(&tab_id).cloned(), false);
        }
        self.recompute(tab_id, now_ms)
    }

    /// Handles a frame navigating away or unloading (`ZendiaCaptchaWatch.drop`).
    /// Returns `(merged_record, changed_announcement)`.
    pub fn drop_frame(&mut self, tab_id: TabId, frame_id: u64, now_ms: u64) -> (Option<CaptchaWatchRecord>, bool) {
        self.sweep_tab_expired(tab_id, now_ms);
        let Some(frames) = self.frames.get_mut(&tab_id) else {
            return (None, false);
        };
        let Some(gone) = frames.get(&frame_id) else {
            return (self.last.get(&tab_id).cloned(), false);
        };
        if gone.leaving_until_ms.is_some() {
            return (self.last.get(&tab_id).cloned(), false);
        }
        let gone_top = gone.top;
        let gone_has_state = gone.state.state != "none";
        let page_stays = frames.iter().any(|(&id, entry)| entry.top && id != frame_id);
        if !gone_top && page_stays && gone_has_state {
            if let Some(entry) = frames.get_mut(&frame_id) {
                entry.leaving_until_ms = Some(now_ms.saturating_add(REPLACE_WAIT_MS));
            }
            return (self.last.get(&tab_id).cloned(), false);
        }
        frames.remove(&frame_id);
        if gone_top {
            frames.retain(|_, entry| entry.leaving_until_ms.is_none());
        }
        self.recompute(tab_id, now_ms)
    }

    /// Expires any deferred subframe drops whose `REPLACE_WAIT_MS` grace window elapsed.
    pub fn sweep_expired_drops(&mut self, now_ms: u64) -> Vec<(TabId, Option<CaptchaWatchRecord>)> {
        let affected: Vec<TabId> = self
            .frames
            .iter()
            .filter_map(|(&tab_id, frames)| {
                frames
                    .values()
                    .any(|e| e.leaving_until_ms.is_some_and(|deadline| now_ms >= deadline))
                    .then_some(tab_id)
            })
            .collect();
        let mut updates = Vec::new();
        for tab_id in affected {
            self.sweep_tab_expired(tab_id, now_ms);
            let (record, changed) = self.recompute(tab_id, now_ms);
            if changed {
                updates.push((tab_id, record));
            }
        }
        updates
    }

    /// Clears all watch state for `tab_id` (on tab close or top-level navigation).
    pub fn clear_tab(&mut self, tab_id: TabId) -> bool {
        self.frames.remove(&tab_id);
        let had_last = self.last.remove(&tab_id).is_some();
        let had_announced = self.announced.remove(&tab_id).is_some();
        had_last || had_announced
    }

    fn sweep_tab_expired(&mut self, tab_id: TabId, now_ms: u64) {
        if let Some(frames) = self.frames.get_mut(&tab_id) {
            frames.retain(|_, entry| entry.leaving_until_ms.is_none_or(|deadline| now_ms < deadline));
        }
    }

    fn recompute(&mut self, tab_id: TabId, now_ms: u64) -> (Option<CaptchaWatchRecord>, bool) {
        let mut merged: Vec<CaptchaWatchDetection> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut url = String::new();
        let mut best_state = CaptchaWatchStateReading {
            state: "none".to_string(),
            name: None,
            how: None,
        };
        if let Some(frames) = self.frames.get(&tab_id) {
            for entry in frames.values() {
                if entry.top || url.is_empty() {
                    url = entry.url.clone();
                }
                if entry.state.state != best_state.state
                    && stronger(&best_state.state, &entry.state.state) == entry.state.state
                {
                    best_state = entry.state.clone();
                }
                for detection in &entry.detections {
                    let key = signature_of_detections(std::slice::from_ref(detection));
                    if seen.insert(key) {
                        merged.push(detection.clone());
                    }
                }
            }
        }
        if merged.is_empty() && best_state.state == "none" {
            self.last.remove(&tab_id);
            let was_announced = self.announced.remove(&tab_id).is_some();
            return (None, was_announced);
        }
        let desc = describe_detections(&merged);
        let text = if !desc.is_empty() {
            desc
        } else {
            best_state
                .name
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "CAPTCHA".to_string())
        };
        let signature = format!("{}#{}", signature_of_detections(&merged), best_state.state);
        let record = CaptchaWatchRecord {
            url,
            detections: merged,
            state: best_state.state,
            how: best_state.how,
            text,
            signature: signature.clone(),
            at_ms: now_ms,
        };
        self.last.insert(tab_id, record.clone());
        let changed = self.announced.get(&tab_id).map(String::as_str) != Some(signature.as_str());
        if changed {
            self.announced.insert(tab_id, signature);
        }
        (Some(record), changed)
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Keyboard Cursor (`PointerPad`) State Machine (`ZendiaPointerContent.sys.mjs:750-1470`)
// ---------------------------------------------------------------------------------------------------------------------

/// Layout kind cycled with `<Tab>` in the keyboard cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadLayoutKind {
    Board,
    Ruler,
    Grid,
    FineGrid,
}

impl PadLayoutKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Board => "board",
            Self::Ruler => "ruler",
            Self::Grid => "grid",
            Self::FineGrid => "fine",
        }
    }
}

/// One jump layout of the keyboard cursor (`board`, `ruler`, `grid`, or `fine`).
#[derive(Clone, Debug, PartialEq)]
pub struct PadLayout {
    pub kind: PadLayoutKind,
    pub rects: Vec<ViewportRect>,
    pub points: Vec<(f32, f32)>,
    pub labels: Vec<String>,
}

/// Options used to open a [`PointerPadState`].
#[derive(Clone, Debug)]
pub struct PointerPadOptions {
    pub areas: Vec<ViewportRect>,
    pub viewport: (f32, f32),
    pub drag: bool,
    pub grab: Option<(f32, f32)>,
    pub grab_rect: Option<ViewportRect>,
    pub slider: bool,
    pub track: Option<ViewportRect>,
    pub piece: Option<ViewportRect>,
    pub lattice: Option<Vec<ViewportRect>>,
    pub banner: String,
    pub can_verify: bool,
    pub watches_captcha: bool,
    pub standalone: bool,
    /// When true (e.g. clicking into a CAPTCHA picture/click-order challenge), lowercase labels click immediately
    /// while recording numbered order markers (`1`, `2`, `3`, …) and prefer the fine `22 px` grid when it fits.
    pub click_order: bool,
}

/// Outcome of feeding a key into [`PointerPadState`].
#[derive(Clone, Debug, PartialEq)]
pub struct PadStepOutcome {
    pub steps: Vec<PointerStep>,
    pub flash_click: bool,
    pub check_soon: bool,
    pub trigger_verify: bool,
    pub trigger_purpose: Option<Vec<&'static str>>,
    pub finished: bool,
}

/// Active keyboard cursor (`PointerPad`) over CAPTCHA areas or the page.
#[derive(Clone, Debug, PartialEq)]
pub struct PointerPadState {
    pub areas: Vec<ViewportRect>,
    pub viewport: (f32, f32),
    pub drag_on_first_jump: bool,
    pub grabbed: bool,
    pub slider: bool,
    /// Track vertical bounds `(top, bottom)` used to keep slider drags on their track.
    pub track_y_bounds: Option<(f32, f32)>,
    /// Horizontal offset from grabbed handle center to puzzle piece center (`0.0` when piece is canvas-drawn).
    pub slider_piece_offset_x: f32,
    /// Vertical span `(top, bottom)` of the ruler lines, used to render a live alignment guide while sliding.
    pub ruler_y_span: Option<(f32, f32)>,
    pub cursor: (f32, f32),
    /// `Some(start_point)` while the primary mouse button is held down.
    pub holding: Option<(f32, f32)>,
    /// Last point reached along a drag path (`#held.walked`).
    pub walked: (f32, f32),
    pub typed: String,
    /// Whether any letter in `typed` was capitalized (`As`, `aS`, `AS` all trigger click/drop).
    pub typed_capital: bool,
    pub count: String,
    /// When true (click-order CAPTCHAs or toggled with `.`), lowercase labels click directly and Capital moves.
    pub click_on_jump: bool,
    /// Ordered coordinates clicked in this session (`1`, `2`, `3`, …), rendered as numbered pins for click-order CAPTCHAs.
    pub clicks: Vec<(f32, f32)>,
    pub layouts: Vec<PadLayout>,
    pub layout_index: usize,
    pub banner: String,
    pub notice: Option<String>,
    pub can_verify: bool,
    pub watches_captcha: bool,
    pub gone_polls: u8,
    /// True when opened directly from `gc` with 0 labelable targets (finishing ends the whole session).
    pub standalone: bool,
}

impl PointerPadState {
    /// Creates a new keyboard cursor and returns `(state, initial_pointer_steps)`.
    pub fn new(mut opts: PointerPadOptions) -> (Self, Vec<PointerStep>) {
        let vw = opts.viewport.0.max(1.0);
        let vh = opts.viewport.1.max(1.0);
        if opts.areas.is_empty() {
            opts.areas.push(ViewportRect {
                x: 0.0,
                y: 0.0,
                width: vw,
                height: vh,
            });
        }
        if let (Some(grab), true) = (opts.grab, opts.slider)
            && let Some(around) = opts
                .areas
                .iter()
                .copied()
                .find(|a| grab.0 >= a.x && grab.0 <= a.x + a.width && grab.1 >= a.y && grab.1 <= a.y + a.height)
        {
            opts.areas = vec![around];
        }
        let cursor = opts.grab.unwrap_or_else(|| middle(opts.areas[0]));
        let slider_piece_offset_x = match (opts.grab, opts.piece) {
            (Some(grab), Some(piece)) => middle(piece).0 - grab.0,
            _ => 0.0,
        };
        let track_y_bounds = if opts.slider {
            opts.track
                .or(opts.grab_rect)
                .map(|r| (r.y, (r.y + r.height).max(r.y + 1.0)))
        } else {
            None
        };
        let mut ruler_y_span = None;
        let mut layouts = Vec::new();
        if let Some(board_rects) = opts.lattice.filter(|l| !l.is_empty()) {
            layouts.push(make_layout(PadLayoutKind::Board, board_rects, None));
        }
        if let (Some(grab), true, Some(grab_rect)) = (opts.grab, opts.slider, opts.grab_rect)
            && let Some(ruler) = build_ruler_layout(&opts.areas, grab, grab_rect, opts.track, opts.piece)
        {
            if let Some(first) = ruler.rects.first() {
                let r_top = first.y;
                let r_bottom = first.y + first.height;
                ruler_y_span = Some((r_top, r_bottom));
                // Ensure the slider's dashed area encloses both the track/handle and the puzzle image above it,
                // even when opened from `s` or when a widget selector only matched the handle itself.
                let base = opts.areas[0];
                let along = opts.track.unwrap_or(grab_rect);
                let left = base.x.min(along.x).min(grab_rect.x);
                let right = (base.x + base.width)
                    .max(along.x + along.width)
                    .max(grab_rect.x + grab_rect.width);
                let top = base.y.min(r_top).max(0.0);
                let bottom = (base.y + base.height)
                    .max(along.y + along.height)
                    .max(grab_rect.y + grab_rect.height)
                    .max(r_bottom)
                    .min(vh);
                opts.areas[0] = ViewportRect {
                    x: left,
                    y: top,
                    width: (right - left).max(1.0),
                    height: (bottom - top).max(1.0),
                };
            }
            layouts.push(ruler);
        }
        let grid_layout = build_grid_layout(&opts.areas);
        let fine_layout = build_fine_grid_layout(&opts.areas, grid_layout.rects.len());
        if opts.click_order
            && let Some(fine) = fine_layout
        {
            layouts.push(fine);
            layouts.push(grid_layout);
        } else {
            layouts.push(grid_layout);
            if let Some(fine) = fine_layout {
                layouts.push(fine);
            }
        }

        let mut initial_steps = Vec::new();
        let (grabbed, holding) = if opts.grab.is_some() {
            initial_steps.push(PointerStep {
                kind: PointerEventKind::Move,
                x: f64::from(cursor.0),
                y: f64::from(cursor.1),
                buttons: 0,
                touch: None,
                pause_ms: 0,
            });
            initial_steps.push(PointerStep {
                kind: PointerEventKind::Down,
                x: f64::from(cursor.0),
                y: f64::from(cursor.1),
                buttons: 1,
                touch: Some(TouchStage::Start),
                pause_ms: 0,
            });
            (true, Some(cursor))
        } else {
            initial_steps.push(PointerStep {
                kind: PointerEventKind::Move,
                x: f64::from(cursor.0),
                y: f64::from(cursor.1),
                buttons: 0,
                touch: None,
                pause_ms: 0,
            });
            (false, None)
        };

        (
            Self {
                areas: opts.areas,
                viewport: (vw, vh),
                drag_on_first_jump: opts.drag,
                grabbed,
                slider: opts.slider,
                track_y_bounds,
                slider_piece_offset_x,
                ruler_y_span,
                cursor,
                holding,
                walked: cursor,
                typed: String::new(),
                typed_capital: false,
                count: String::new(),
                click_on_jump: opts.click_order && !opts.drag && opts.grab.is_none(),
                clicks: Vec::new(),
                layouts,
                layout_index: 0,
                banner: opts.banner,
                notice: None,
                can_verify: opts.can_verify,
                watches_captcha: opts.watches_captcha,
                gone_polls: 0,
                standalone: opts.standalone,
            },
            initial_steps,
        )
    }

    /// Currently displayed layout (`None` when `<Tab>` cycled to `"hide"`).
    pub fn shown_layout(&self) -> Option<&PadLayout> {
        self.layouts.get(self.layout_index)
    }

    /// Horizontal pixel delta from where the current hold started, when dragging a slider.
    pub fn slider_delta_px(&self) -> Option<i32> {
        if self.slider
            && let Some(start) = self.holding
        {
            Some((self.cursor.0 - start.0).round() as i32)
        } else {
            None
        }
    }

    /// Any release step needed if the pad is torn down while holding the mouse button (`ZendiaPointerContent:1254`).
    pub fn release_if_held(&mut self) -> Vec<PointerStep> {
        if self.holding.take().is_some() {
            vec![PointerStep {
                kind: PointerEventKind::Up,
                x: f64::from(self.cursor.0),
                y: f64::from(self.cursor.1),
                buttons: 0,
                touch: Some(TouchStage::End),
                pause_ms: 0,
            }]
        } else {
            Vec::new()
        }
    }

    /// Exact prompt text rendered by the cursor (`ZendiaPointerContent.sys.mjs:1315-1358`, `CAP-09`).
    pub fn prompt_text(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if !self.banner.is_empty() {
            parts.push(self.banner.clone());
        }
        if let Some(notice) = self.notice.as_deref()
            && !notice.is_empty()
        {
            parts.push(notice.to_string());
        }
        if !self.count.is_empty() {
            parts.push(self.count.clone());
        }
        if let Some(dx) = self.slider_delta_px()
            && dx != 0
        {
            parts.push(if dx > 0 { format!("+{dx}px") } else { format!("{dx}px") });
        }
        if !self.clicks.is_empty() && self.holding.is_none() {
            let n = self.clicks.len();
            parts.push(format!("{n} click{}", if n == 1 { "" } else { "s" }));
        }
        let kind = self.shown_layout().map(|l| l.kind);
        if self.holding.is_some() {
            let head = match kind {
                Some(PadLayoutKind::Ruler) => "slide · letter puts the piece on its line · Letter drops",
                Some(PadLayoutKind::Board) => "holding · label puts it there",
                _ => "holding · label drags there · Label drops",
            };
            parts.push(head.to_string());
            parts.push("hjkl HJKL ←→ move".to_string());
            if self.slider {
                parts.push("0 start".to_string());
            }
            parts.push("Space/Enter drop".to_string());
            parts.push("Esc undo".to_string());
        } else {
            parts.push(
                if self.drag_on_first_jump {
                    "drag · label takes hold there"
                } else if self.click_on_jump {
                    "click-order · label clicks · Label moves · . mode"
                } else {
                    "cursor · label moves · Label clicks · . order"
                }
                .to_string(),
            );
            parts.push("hjkl HJKL ←→ move".to_string());
            parts.push("Space click".to_string());
            parts.push("v hold".to_string());
            if self.can_verify {
                parts.push("Enter verify".to_string());
            }
            parts.push("Esc labels".to_string());
        }
        if !self.layouts.is_empty() {
            let next = self
                .layouts
                .get(self.layout_index + 1)
                .map_or("hide", |l| l.kind.name());
            parts.push(format!("Tab {next}"));
        }
        parts.join(" · ")
    }

    /// Processes a single key token (`<Esc>`, `<BS>`, `<Tab>`, `<Space>`, `<CR>`, `<Left>`, `<C-r>`, `h`, `9`, `A`, `.`, …).
    pub fn handle_token(&mut self, token: &str, staying: bool) -> PadStepOutcome {
        let mut out = PadStepOutcome {
            steps: Vec::new(),
            flash_click: false,
            check_soon: false,
            trigger_verify: false,
            trigger_purpose: None,
            finished: false,
        };
        if staying {
            let purpose = match token {
                "<C-r>" => Some(vec!["reload"]),
                "<C-a>" => Some(vec!["audio", "visual"]),
                "<C-p>" => Some(vec!["play"]),
                _ => None,
            };
            if let Some(purposes) = purpose {
                out.steps.extend(self.release_if_held());
                out.trigger_purpose = Some(purposes);
                out.finished = true;
                return out;
            }
        }
        match token {
            "<Esc>" => {
                if !self.typed.is_empty() || !self.count.is_empty() {
                    self.typed.clear();
                    self.typed_capital = false;
                    self.count.clear();
                } else if let Some(start) = self.holding {
                    self.move_to(start, &mut out.steps);
                    self.let_go(true, &mut out);
                } else {
                    out.finished = true;
                }
            }
            "<BS>" => {
                if !self.typed.is_empty() {
                    self.typed.pop();
                    if self.typed.is_empty() {
                        self.typed_capital = false;
                    }
                } else if !self.count.is_empty() {
                    self.count.clear();
                } else if self.holding.is_none() {
                    self.clicks.pop();
                }
            }
            "<Tab>" => {
                self.typed.clear();
                self.typed_capital = false;
                self.layout_index = (self.layout_index + 1) % (self.layouts.len() + 1);
            }
            "." if self.holding.is_none() => {
                self.typed.clear();
                self.typed_capital = false;
                self.count.clear();
                self.drag_on_first_jump = false;
                self.click_on_jump = !self.click_on_jump;
            }
            "<Space>" | "<CR>" => {
                self.count.clear();
                if self.holding.is_some() {
                    self.let_go(false, &mut out);
                } else if token == "<CR>" && self.can_verify {
                    out.steps.extend(self.release_if_held());
                    out.trigger_verify = true;
                    out.finished = true;
                } else {
                    self.click(&mut out);
                }
            }
            "v" => {
                self.count.clear();
                if self.holding.is_some() {
                    self.let_go(false, &mut out);
                } else {
                    self.start_hold(&mut out.steps);
                }
            }
            _ if step_delta(token).is_some() => {
                let (dx, dy) = step_delta(token).unwrap();
                let times = self.count.parse::<u32>().unwrap_or(1).max(1) as f32;
                self.count.clear();
                self.move_to((self.cursor.0 + dx * times, self.cursor.1 + dy * times), &mut out.steps);
            }
            _ => {
                let mut chars = token.chars();
                if let (Some(ch), None) = (chars.next(), chars.next()) {
                    if ch == '0' && self.count.is_empty() && self.typed.is_empty() {
                        if let Some(start) = self.holding {
                            self.move_to(start, &mut out.steps);
                        }
                    } else if ch.is_ascii_digit() && self.typed.is_empty() {
                        self.count = grow_count(&self.count, ch, 999);
                    } else {
                        self.label_key(ch, &mut out);
                    }
                }
            }
        }
        out
    }

    fn label_key(&mut self, ch: char, out: &mut PadStepOutcome) {
        if self.layouts.is_empty() {
            return;
        }
        let active_idx = if self.layout_index < self.layouts.len() {
            self.layout_index
        } else {
            self.layouts.len() - 1
        };
        let lower: String = ch.to_lowercase().collect();
        let ch_capital = lower != ch.to_string();
        let typed = format!("{}{lower}", self.typed);
        self.count.clear();
        if let Some(item_idx) = self.layouts[active_idx].labels.iter().position(|l| *l == typed) {
            let capital = self.typed_capital || ch_capital;
            self.typed.clear();
            self.typed_capital = false;
            let at = self.layouts[active_idx].points[item_idx];
            let kind = self.layouts[active_idx].kind;
            self.move_to(at, &mut out.steps);
            if self.holding.is_some() {
                if capital || kind == PadLayoutKind::Board {
                    self.let_go(false, out);
                }
            } else if self.drag_on_first_jump && !capital {
                self.drag_on_first_jump = false;
                self.start_hold(&mut out.steps);
            } else if self.click_on_jump {
                if !capital {
                    self.click(out);
                }
            } else if capital {
                self.click(out);
            }
        } else if self.layouts[active_idx].labels.iter().any(|l| l.starts_with(&typed)) {
            self.typed = typed;
            self.typed_capital |= ch_capital;
            if self.layout_index >= self.layouts.len() {
                self.layout_index = active_idx;
            }
        }
    }

    fn move_to(&mut self, point: (f32, f32), steps: &mut Vec<PointerStep>) {
        let max_x = (self.viewport.0 - 1.0).max(0.0);
        let max_y = (self.viewport.1 - 1.0).max(0.0);
        let clamped_y = if self.holding.is_some()
            && let Some((min_y, track_max_y)) = self.track_y_bounds
        {
            point.1.clamp(min_y.max(0.0), track_max_y.min(max_y))
        } else {
            point.1.clamp(0.0, max_y)
        };
        let to = (point.0.clamp(0.0, max_x), clamped_y);
        let from = self.cursor;
        self.cursor = to;
        if self.holding.is_some() {
            let drag_from = self.walked;
            for next in path(drag_from, to, PATH_STEP) {
                steps.push(PointerStep {
                    kind: PointerEventKind::Move,
                    x: f64::from(next.0),
                    y: f64::from(next.1),
                    buttons: 1,
                    touch: Some(TouchStage::Move),
                    pause_ms: MOVE_PAUSE_MS,
                });
            }
            self.walked = to;
        } else {
            for next in path(from, to, PATH_STEP * 3.0) {
                steps.push(PointerStep {
                    kind: PointerEventKind::Move,
                    x: f64::from(next.0),
                    y: f64::from(next.1),
                    buttons: 0,
                    touch: None,
                    pause_ms: 0,
                });
            }
        }
    }

    fn start_hold(&mut self, steps: &mut Vec<PointerStep>) {
        let at = self.cursor;
        self.holding = Some(at);
        self.walked = at;
        steps.push(PointerStep {
            kind: PointerEventKind::Move,
            x: f64::from(at.0),
            y: f64::from(at.1),
            buttons: 0,
            touch: None,
            pause_ms: 0,
        });
        steps.push(PointerStep {
            kind: PointerEventKind::Down,
            x: f64::from(at.0),
            y: f64::from(at.1),
            buttons: 1,
            touch: Some(TouchStage::Start),
            pause_ms: 0,
        });
    }

    fn let_go(&mut self, force_end: bool, out: &mut PadStepOutcome) {
        let over = force_end || self.grabbed;
        if self.holding.take().is_some() {
            let at = self.cursor;
            if (self.walked.0 - at.0).abs() > 0.01 || (self.walked.1 - at.1).abs() > 0.01 {
                for next in path(self.walked, at, PATH_STEP) {
                    out.steps.push(PointerStep {
                        kind: PointerEventKind::Move,
                        x: f64::from(next.0),
                        y: f64::from(next.1),
                        buttons: 1,
                        touch: Some(TouchStage::Move),
                        pause_ms: MOVE_PAUSE_MS,
                    });
                }
                self.walked = at;
            }
            out.steps.push(PointerStep {
                kind: PointerEventKind::Up,
                x: f64::from(at.0),
                y: f64::from(at.1),
                buttons: 0,
                touch: Some(TouchStage::End),
                pause_ms: 0,
            });
        }
        if over {
            out.finished = true;
        } else {
            out.check_soon = true;
        }
    }

    fn click(&mut self, out: &mut PadStepOutcome) {
        let at = self.cursor;
        self.clicks.push(at);
        self.notice = Some(format!("click #{}", self.clicks.len()));
        out.flash_click = true;
        out.check_soon = true;
        out.steps.push(PointerStep {
            kind: PointerEventKind::Move,
            x: f64::from(at.0),
            y: f64::from(at.1),
            buttons: 0,
            touch: None,
            pause_ms: 0,
        });
        out.steps.push(PointerStep {
            kind: PointerEventKind::Down,
            x: f64::from(at.0),
            y: f64::from(at.1),
            buttons: 1,
            touch: None,
            pause_ms: 0,
        });
        out.steps.push(PointerStep {
            kind: PointerEventKind::Up,
            x: f64::from(at.0),
            y: f64::from(at.1),
            buttons: 0,
            touch: None,
            pause_ms: 0,
        });
    }
}

fn step_delta(token: &str) -> Option<(f32, f32)> {
    match token {
        "h" => Some((-STEP, 0.0)),
        "l" => Some((STEP, 0.0)),
        "k" => Some((0.0, -STEP)),
        "j" => Some((0.0, STEP)),
        "H" | "<S-Left>" => Some((-FAR_STEP, 0.0)),
        "L" | "<S-Right>" => Some((FAR_STEP, 0.0)),
        "K" | "<S-Up>" => Some((0.0, -FAR_STEP)),
        "J" | "<S-Down>" => Some((0.0, FAR_STEP)),
        "<Left>" => Some((-FINE_STEP, 0.0)),
        "<Right>" => Some((FINE_STEP, 0.0)),
        "<Up>" => Some((0.0, -FINE_STEP)),
        "<Down>" => Some((0.0, FINE_STEP)),
        _ => None,
    }
}

fn make_layout(kind: PadLayoutKind, rects: Vec<ViewportRect>, points: Option<Vec<(f32, f32)>>) -> PadLayout {
    let pts = points.unwrap_or_else(|| rects.iter().copied().map(middle).collect());
    let labels = labels_of(rects.len(), PAD_LETTERS);
    PadLayout {
        kind,
        rects,
        points: pts,
        labels,
    }
}

fn build_grid_layout(areas: &[ViewportRect]) -> PadLayout {
    let mut size = CELL_SIZE;
    let cells = loop {
        let candidate: Vec<ViewportRect> = areas.iter().copied().flat_map(|a| grid_cells(a, size)).collect();
        if candidate.len() <= MAX_CELLS {
            break candidate;
        }
        size *= 1.5;
    };
    make_layout(PadLayoutKind::Grid, cells, None)
}

fn build_fine_grid_layout(areas: &[ViewportRect], coarse_count: usize) -> Option<PadLayout> {
    let cells: Vec<ViewportRect> = areas
        .iter()
        .copied()
        .flat_map(|a| grid_cells(a, FINE_CELL_SIZE))
        .collect();
    if cells.len() <= coarse_count || cells.len() > MAX_CELLS {
        return None;
    }
    Some(make_layout(PadLayoutKind::FineGrid, cells, None))
}

fn build_ruler_layout(
    areas: &[ViewportRect],
    grab: (f32, f32),
    handle: ViewportRect,
    track: Option<ViewportRect>,
    piece: Option<ViewportRect>,
) -> Option<PadLayout> {
    let area = areas
        .iter()
        .copied()
        .find(|each| {
            rects_meet(
                *each,
                ViewportRect {
                    x: grab.0,
                    y: grab.1,
                    width: 1.0,
                    height: 1.0,
                },
            )
        })
        .unwrap_or(areas[0]);
    let along = track.unwrap_or(area);
    let from = piece.map(middle).unwrap_or(grab);
    let stops = ruler_stops(
        from.0,
        handle.x,
        handle.width,
        along.x,
        along.width,
        PAD_LETTERS.chars().count(),
    );
    if stops.is_empty() {
        return None;
    }
    let (mut top, mut bottom) = if let Some(p) = piece {
        (p.y - RULER_LABEL_ROOM, p.y + p.height)
    } else {
        let b = handle.y.min(track.map_or(handle.y, |t| t.y)) - 2.0;
        let t = b - along.width * RULER_HEIGHT;
        (t, b)
    };
    // Only clamp `top` to `area.y` when `area` actually reaches above the slider into the puzzle image.
    // If `area` only wraps the slider handle/track, never crush the ruler down onto the handle.
    if area.y < bottom - 40.0 {
        top = top.max(area.y);
    }
    top = top.max(0.0);
    if bottom <= top + 20.0 {
        bottom = (top + 20.0).min(handle.y + handle.height);
    }
    let height = (bottom - top).max(20.0);
    let rects: Vec<ViewportRect> = stops
        .iter()
        .map(|&x| ViewportRect {
            x,
            y: top,
            width: 1.0,
            height,
        })
        .collect();
    let points: Vec<(f32, f32)> = stops.iter().map(|&x| (grab.0 + (x - from.0), grab.1)).collect();
    Some(make_layout(PadLayoutKind::Ruler, rects, Some(points)))
}

/// Default hint alphabet re-export for sticky labels.
pub fn default_hint_alphabet() -> &'static str {
    HINT_ALPHABET
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALPHABET: &str = "asdfghlqweruiopzxcvbnmty";

    #[test]
    fn names_the_service_of_a_captcha_frame() {
        assert_eq!(
            name_of("https://www.google.com/recaptcha/api2/bframe?hl=en"),
            Some("reCAPTCHA")
        );
        assert_eq!(
            name_of("https://newassets.hcaptcha.com/captcha/v1/x/static/hcaptcha.html"),
            Some("hCaptcha")
        );
        assert_eq!(
            name_of("https://client-api.arkoselabs.com/fc/assets/ec-game-core"),
            Some("Arkose")
        );
        assert_eq!(name_of("https://example.com/login"), None);
    }

    #[test]
    fn a_challenge_is_the_frame_that_asks_for_clicks() {
        assert!(is_challenge("https://www.google.com/recaptcha/api2/bframe?hl=en"));
        assert!(is_challenge("https://www.google.com/recaptcha/enterprise/bframe?hl=en"));
        assert!(!is_challenge("https://www.google.com/recaptcha/api2/anchor?k=1"));
        assert!(is_challenge(
            "https://newassets.hcaptcha.com/captcha/v1/x/static/hcaptcha.html#frame=challenge&id=1"
        ));
        assert!(!is_challenge(
            "https://newassets.hcaptcha.com/captcha/v1/x/static/hcaptcha.html#frame=checkbox&id=1"
        ));
        assert!(is_challenge("https://client-api.arkoselabs.com/fc/assets/ec-game-core"));
        assert!(!is_challenge(
            "https://challenges.cloudflare.com/cdn-cgi/challenge-platform/turnstile"
        ));
        assert!(!is_challenge("https://example.com/"));
    }

    #[test]
    fn labels_stay_with_their_targets() {
        let kept = BTreeMap::from([
            ("x".to_string(), "aa".to_string()),
            ("y".to_string(), "as".to_string()),
            ("z".to_string(), "ad".to_string()),
        ]);
        let keys = vec!["y".to_string(), "z".to_string(), "w".to_string()];
        let labels: Vec<String> = sticky_labels(&keys, &kept, ALPHABET).into_iter().flatten().collect();
        assert_eq!(labels[0], "as");
        assert_eq!(labels[1], "ad");
        assert_eq!(labels[2].len(), 2);
        assert!(!["as", "ad"].contains(&labels[2].as_str()));
        assert_eq!(labels.iter().collect::<BTreeSet<_>>().len(), 3);
    }

    #[test]
    fn labels_start_short_when_few() {
        let keys = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let labels = sticky_labels(&keys, &BTreeMap::new(), ALPHABET);
        assert_eq!(
            labels,
            vec![Some("a".to_string()), Some("s".to_string()), Some("d".to_string())]
        );
    }

    #[test]
    fn a_label_is_never_given_twice() {
        let kept = BTreeMap::from([("x".to_string(), "a".to_string()), ("y".to_string(), "a".to_string())]);
        let keys = vec!["x".to_string(), "y".to_string()];
        let labels: Vec<String> = sticky_labels(&keys, &kept, ALPHABET).into_iter().flatten().collect();
        assert_eq!(labels.iter().collect::<BTreeSet<_>>().len(), 2);
    }

    #[test]
    fn a_grid_fills_the_area() {
        let cells = grid_cells(
            ViewportRect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 80.0,
            },
            40.0,
        );
        assert_eq!(cells.len(), 3 * 2);
        assert_eq!((cells[0].x, cells[0].y), (10.0, 20.0));
        let last = cells.last().unwrap();
        assert!((last.x + last.width - 110.0).abs() < 0.001);
        assert!((last.y + last.height - 100.0).abs() < 0.001);
        assert_eq!(
            middle(ViewportRect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 6.0
            }),
            (5.0, 3.0)
        );
    }

    #[test]
    fn a_drag_is_a_path_of_small_steps() {
        let p = path((0.0, 0.0), (50.0, 0.0), 12.0);
        assert_eq!(p.len(), 5);
        assert_eq!(*p.last().unwrap(), (50.0, 0.0));
        for (i, point) in p.iter().enumerate() {
            let prev_x = if i == 0 { 0.0 } else { p[i - 1].0 };
            assert!(point.0 - prev_x <= 12.001);
        }
        assert_eq!(path((3.0, 3.0), (3.0, 3.0), 12.0), vec![(3.0, 3.0)]);
    }

    #[test]
    fn more_services_are_named() {
        for (url, expected) in [
            ("https://api.geetest.com/get.php", "GeeTest"),
            ("https://geo.captcha-delivery.com/captcha/?initialCid=1", "DataDome"),
            ("https://captcha.px-cdn.net/PX1/captcha.js", "HUMAN"),
            ("https://smartcaptcha.yandexcloud.net/webtest", "Yandex SmartCaptcha"),
            ("https://octocaptcha.com/?origin=github", "Arkose"),
            ("https://api.friendlycaptcha.com/api/v1/puzzle", "Friendly Captcha"),
        ] {
            assert_eq!(name_of(url), Some(expected), "{url}");
        }
        assert_eq!(name_of("https://www.hcaptcha.com/privacy"), None);
    }

    #[test]
    fn a_board_is_found_from_its_pieces() {
        let piece = |col: usize, row: usize| ViewportRect {
            x: 100.0 + col as f32 * 50.0,
            y: 40.0 + row as f32 * 50.0,
            width: 40.0,
            height: 40.0,
        };
        let places = lattice(&[piece(0, 0), piece(1, 1), piece(4, 4)]).expect("5x5 board");
        assert_eq!(places.len(), 25);
        assert_eq!(places[0], piece(0, 0));
        assert_eq!(places[24], piece(4, 4));
        assert!(lattice(&[piece(0, 0), piece(1, 1)]).is_none());
        let mut off = piece(2, 0);
        off.x = 215.0;
        assert!(lattice(&[piece(0, 0), piece(1, 0), off]).is_none());
    }

    #[test]
    fn turnstile_and_check_kinds_are_told() {
        assert_eq!(
            name_of("https://example.com/cdn-cgi/challenge-platform/h/b/turnstile/if/ov2/av0"),
            Some("Cloudflare Turnstile")
        );
        for (url, expected) in [
            ("https://www.google.com/recaptcha/api2/bframe?hl=en", "image"),
            ("https://www.google.com/recaptcha/api2/anchor?k=1", "checkbox"),
            (
                "https://www.google.com/recaptcha/api2/api.js?render=explicit",
                "checkbox",
            ),
            (
                "https://www.google.com/recaptcha/api2/api.js?onload=cb&render=6Lcx",
                "score",
            ),
            ("https://www.google.com/recaptcha/enterprise.js?render=6Lcx", "score"),
            (
                "https://challenges.cloudflare.com/cdn-cgi/challenge-platform/turnstile",
                "managed",
            ),
            ("https://client-api.arkoselabs.com/fc/assets/ec-game-core", "game"),
            ("https://api.geetest.com/get.php", "slider"),
            ("https://geo.captcha-delivery.com/captcha/?initialCid=1", "slider"),
            ("https://captcha.px-cdn.net/PX1/captcha.js", "behavioral"),
            ("https://api.friendlycaptcha.com/api/v1/puzzle", "proof-of-work"),
            ("https://service.mtcaptcha.com/mtcv1/demo/abc", "text"),
            ("https://smartcaptcha.yandexcloud.net/webtest", "checkbox"),
        ] {
            assert_eq!(kind_of(url), Some(expected), "{url}");
            assert!(KINDS.contains(&expected));
        }
        assert_eq!(kind_of("https://example.com/"), None);
    }

    #[test]
    fn scripts_give_away_the_invisible_ones() {
        for (url, name, kind) in [
            (
                "https://www.google.com/recaptcha/api2/api.js?render=6Lcx",
                "reCAPTCHA",
                "score",
            ),
            (
                "https://www.gstatic.com/recaptcha/releases/abc/recaptcha__en.js",
                "reCAPTCHA",
                "checkbox",
            ),
            ("https://js.hcaptcha.com/1/api.js", "hCaptcha", "checkbox"),
            (
                "https://challenges.cloudflare.com/turnstile/v0/api.js",
                "Cloudflare Turnstile",
                "managed",
            ),
            ("https://js.datadome.co/tags.js", "DataDome", "behavioral"),
            ("https://example.com/ips.js", "Kasada", "sensor"),
            (
                "https://cdn.jsdelivr.net/npm/altcha@2/dist/altcha.min.js",
                "ALTCHA",
                "proof-of-work",
            ),
            (
                "https://cdn.jsdelivr.net/npm/friendly-challenge@0.9/widget.module.min.js",
                "Friendly Captcha",
                "proof-of-work",
            ),
        ] {
            assert_eq!(name_of_script(url), Some(name), "{url}");
            assert_eq!(kind_of_script(url), Some(kind), "{url}");
            assert_eq!(kind_of(url), Some(kind), "{url}");
        }
        assert_eq!(name_of_script("https://example.com/app.js"), None);
        assert_eq!(kind_of_script("https://example.com/app.js"), None);
    }

    #[test]
    fn tokens_honeypots_purposes_and_states_match_fork_tests() {
        for name in [
            "g-recaptcha-response",
            "g-recaptcha-response-1",
            "h-captcha-response",
            "cf-turnstile-response",
            "frc-captcha-solution",
            "mtcaptcha-verifiedtoken",
            "aws-waf-token",
            "fc-token",
            "altcha",
            "geetest_validate",
            "lemin_answer",
        ] {
            assert!(is_token_field(name), "{name}");
        }
        for name in ["username", "csrfmiddlewaretoken", "g-recaptcha"] {
            assert!(!is_token_field(name), "{name}");
        }

        assert!(is_honeypot_field("website", false));
        assert!(is_honeypot_field("hp", false));
        assert!(!is_honeypot_field("website", true));
        assert!(!is_honeypot_field("email", false));

        let btn = |label: &'static str, name: &'static str| {
            purpose_of(ControlInfo {
                tag: "button",
                label,
                name,
                ..Default::default()
            })
        };
        assert_eq!(
            btn("Get a new challenge", "recaptcha-reload-button rc-button"),
            Some("reload")
        );
        assert_eq!(btn("Get an audio challenge", "recaptcha-audio-button"), Some("audio"));
        assert_eq!(btn("Get a visual challenge", "recaptcha-image-button"), Some("visual"));
        assert_eq!(btn("SKIP", "recaptcha-verify-button"), Some("verify"));
        assert_eq!(btn("Verify", ""), Some("verify"));
        assert_eq!(btn("Next", ""), Some("verify"));
        assert_eq!(
            btn("PLAY", "rc-button-default rc-audiochallenge-play-button"),
            Some("play")
        );
        assert_eq!(btn("Restart", ""), Some("reload"));
        assert_eq!(btn("", "refresh button"), Some("reload"));
        assert_eq!(btn("Reset", ""), Some("reload"));
        assert_eq!(btn("Check", ""), Some("verify"));
        assert_eq!(btn("Rotate left", ""), None);
        assert_eq!(btn("Help", "recaptcha-help-button"), None);

        assert_eq!(
            purpose_of(ControlInfo {
                tag: "input",
                r#type: "text",
                editable: true,
                name: "audio-response",
                ..Default::default()
            }),
            Some("answer")
        );
        assert_eq!(
            purpose_of(ControlInfo {
                tag: "select",
                editable: true,
                ..Default::default()
            }),
            None
        );
        assert_eq!(
            purpose_of(ControlInfo {
                tag: "input",
                r#type: "checkbox",
                label: "Verify you are human",
                ..Default::default()
            }),
            None
        );

        assert!(names_own_captcha(
            "/dist/web/assets/captcha-rn1S3orp.jpg normal captcha example _captchaImage_rrn3u_9"
        ));
        assert!(names_own_captcha("Enter the characters you see"));
        assert!(!names_own_captcha("/assets/normal-captcha.svg Logo of Normal Captcha"));
        assert!(!names_own_captcha("g-recaptcha-response"));

        assert_eq!(labels_of(3, "abc"), vec!["a", "b", "c"]);
        assert_eq!(labels_of(5, "abc"), vec!["aa", "ab", "ac", "ba", "bb"]);
        assert!(labels_of(0, "abc").is_empty());

        assert_eq!(ruler_stops(15.0, 0.0, 30.0, 0.0, 300.0, 3), vec![105.0, 195.0, 285.0]);
        assert_eq!(ruler_stops(40.0, 0.0, 30.0, 0.0, 300.0, 3), vec![130.0, 220.0, 310.0]);
        assert!(ruler_stops(15.0, 280.0, 30.0, 0.0, 300.0, 3).is_empty());

        assert_eq!(grow_count("", '3', 999), "3");
        assert_eq!(grow_count("3", '0', 999), "30");
        assert_eq!(grow_count("", '0', 999), "");
        assert_eq!(grow_count("99", '9', 999), "999");
        assert_eq!(grow_count("999", '9', 999), "999");
        assert_eq!(grow_count("1", 'x', 999), "1");

        assert_eq!(stronger("solved", "unchecked"), "unchecked");
        assert_eq!(stronger("open", "unchecked"), "open");
        assert_eq!(stronger("checking", "solved"), "checking");
        assert_eq!(stronger("none", "solved"), "solved");
        assert_eq!(stronger("bogus", "none"), "none");

        for s in ["open", "unchecked", "failed", "shown"] {
            assert!(asks_user(s) && holds_up(s) && !is_cleared(s));
        }
        assert!(!asks_user("checking") && holds_up("checking"));
        for s in ["solved", "none"] {
            assert!(!holds_up(s) && is_cleared(s));
        }

        assert!(marks_solved(["geetest_lock_success"], ""));
        assert!(marks_solved(["frc-captcha", "frc-success"], ""));
        assert!(marks_solved([], "verified"));
        assert!(!marks_solved(["success-message-box-wrapper"], ""));

        assert_eq!(token_state(&format!("03AFcWeA{}", "x".repeat(400))), Some("solved"));
        assert_eq!(token_state("XXXX.DUMMY.TOKEN.XXXX"), Some("solved"));
        for v in [
            "",
            "  ",
            "null",
            "undefined",
            "false",
            "0",
            ".UNSTARTED",
            ".UNACTIVATED",
        ] {
            assert_eq!(token_state(v), None, "{v}");
        }
        for v in [".EXPIRED", ".ERROR", ".HEADLESS_ERROR", ".RESET"] {
            assert_eq!(token_state(v), Some("failed"), "{v}");
        }
        for v in [".FETCHING", ".UNFINISHED", ".SOLVING"] {
            assert_eq!(token_state(v), Some("checking"), "{v}");
        }
    }

    #[test]
    fn slider_ruler_and_counted_drag_match_fork_scenario() {
        // Mirrors `Slider: gc, A -> ruler of 21 labels; 9L, l, <Space> -> moves 90px + 2px = 92px and drops`.
        let area = ViewportRect {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 200.0,
        };
        let handle = ViewportRect {
            x: 10.0,
            y: 150.0,
            width: 40.0,
            height: 36.0,
        };
        let track = ViewportRect {
            x: 10.0,
            y: 150.0,
            width: 280.0,
            height: 36.0,
        };
        let (mut pad, init) = PointerPadState::new(PointerPadOptions {
            areas: vec![area],
            viewport: (800.0, 600.0),
            drag: true,
            grab: Some(middle(handle)),
            grab_rect: Some(handle),
            slider: true,
            track: Some(track),
            piece: None,
            lattice: None,
            banner: String::new(),
            can_verify: true,
            watches_captcha: true,
            standalone: false,
            click_order: false,
        });
        assert_eq!(init.len(), 2);
        assert_eq!(pad.shown_layout().unwrap().kind, PadLayoutKind::Ruler);
        assert_eq!(pad.shown_layout().unwrap().labels.len(), 21);
        assert!(pad.prompt_text().contains("slide · letter puts the piece on its line"));

        let start_x = pad.cursor.0;
        let start_y = pad.cursor.1;
        pad.handle_token("9", true);
        let step1 = pad.handle_token("L", true);
        assert!(!step1.steps.is_empty());
        let step2 = pad.handle_token("l", true);
        assert!((pad.cursor.0 - (start_x + 92.0)).abs() < 0.01);
        assert!(!step2.steps.is_empty());
        assert!(pad.prompt_text().contains("+92px"));

        // Vertical nudges while sliding stay clamped inside the slider track bounds.
        pad.handle_token("9", true);
        pad.handle_token("J", true);
        assert!(pad.cursor.1 <= track.y + track.height);
        assert!(pad.cursor.1 >= track.y);

        // `0` snaps the slider handle back to its start without releasing.
        let reset = pad.handle_token("0", true);
        assert!(!reset.finished);
        assert!(pad.holding.is_some());
        assert!((pad.cursor.0 - start_x).abs() < 0.01);
        assert!((pad.cursor.1 - start_y).abs() < 0.01);

        // Capital ruler letter slides directly to that stop and drops.
        let cap_drop = pad.handle_token("A", true);
        assert!(cap_drop.finished, "capital ruler letter drops immediately");
        assert_eq!(cap_drop.steps.last().unwrap().kind, PointerEventKind::Up);

        // Even when `areas` only wraps the tiny slider handle (e.g. opened from `s` without outer `captcha_areas`),
        // the ruler must extend upward across the puzzle image rather than crushing onto the handle.
        let handle_only = ViewportRect {
            x: 120.0,
            y: 360.0,
            width: 56.0,
            height: 44.0,
        };
        let track_wide = ViewportRect {
            x: 120.0,
            y: 360.0,
            width: 260.0,
            height: 44.0,
        };
        let (pad_from_handle, _) = PointerPadState::new(PointerPadOptions {
            areas: vec![handle_only],
            viewport: (800.0, 600.0),
            drag: true,
            grab: Some(middle(handle_only)),
            grab_rect: Some(handle_only),
            slider: true,
            track: Some(track_wide),
            piece: None,
            lattice: None,
            banner: String::new(),
            can_verify: true,
            watches_captcha: true,
            standalone: false,
            click_order: false,
        });
        let ruler = pad_from_handle.shown_layout().unwrap();
        assert_eq!(ruler.kind, PadLayoutKind::Ruler);
        let first_line = ruler.rects[0];
        assert!(
            first_line.height >= 150.0,
            "ruler lines should span the puzzle image above the slider, got height={}",
            first_line.height
        );
        assert!(
            first_line.y < handle_only.y - 100.0,
            "ruler tags should sit at the top of the puzzle image, not on the handle"
        );
        assert!(pad_from_handle.areas[0].y <= first_line.y + 0.1);
        assert!(pad_from_handle.areas[0].width >= track_wide.width - 0.1);
    }

    #[test]
    fn click_order_picture_uses_fine_grid_records_ordered_pins_and_toggles_mode() {
        let picture = ViewportRect {
            x: 100.0,
            y: 80.0,
            width: 300.0,
            height: 200.0,
        };
        let (mut pad, _) = PointerPadState::new(PointerPadOptions {
            areas: vec![picture],
            viewport: (800.0, 600.0),
            drag: false,
            grab: None,
            grab_rect: None,
            slider: false,
            track: None,
            piece: None,
            lattice: None,
            banner: String::new(),
            can_verify: true,
            watches_captcha: true,
            standalone: false,
            click_order: true,
        });
        assert_eq!(pad.shown_layout().unwrap().kind, PadLayoutKind::FineGrid);
        assert!(pad.click_on_jump);
        assert!(pad.prompt_text().contains("click-order"));

        let labels: Vec<String> = pad.shown_layout().unwrap().labels.clone();
        let first = labels[0].clone();
        let second = labels[1].clone();
        assert_eq!(first.len(), 2);

        // Lowercase 2-letter label clicks directly and records pin #1.
        for ch in first.chars() {
            let out = pad.handle_token(&ch.to_string(), true);
            assert!(!out.finished);
        }
        assert_eq!(pad.clicks.len(), 1);
        assert!(pad.prompt_text().contains("1 click"));

        // Lowercase second label clicks directly and records pin #2.
        for ch in second.chars() {
            pad.handle_token(&ch.to_string(), true);
        }
        assert_eq!(pad.clicks.len(), 2);
        assert!(pad.prompt_text().contains("2 clicks"));

        // Capital first letter in click-order mode moves without clicking (for fine adjustment).
        let third = labels[2].clone();
        let mut chars = third.chars();
        let first_upper = chars.next().unwrap().to_uppercase().to_string();
        let second_lower = chars.next().unwrap().to_string();
        pad.handle_token(&first_upper, true);
        pad.handle_token(&second_lower, true);
        assert_eq!(
            pad.clicks.len(),
            2,
            "capital label moves without clicking in click-order mode"
        );

        // `<BS>` pops the last visual click marker when nothing is typed.
        pad.handle_token("<BS>", true);
        assert_eq!(pad.clicks.len(), 1);

        // `.` toggles back to move-first mode, where capital first letter (`As`) clicks.
        pad.handle_token(".", true);
        assert!(!pad.click_on_jump);
        pad.handle_token(&first_upper, true);
        let clicked = pad.handle_token(&second_lower, true);
        assert!(clicked.flash_click);
        assert_eq!(pad.clicks.len(), 2);

        // `<CR>` triggers verify and finishes the cursor.
        let verify = pad.handle_token("<CR>", true);
        assert!(verify.trigger_verify && verify.finished);
    }

    #[test]
    fn captcha_watch_service_matches_fork_test_captcha_watch() {
        let det = |signal: &str, name: Option<&str>, kind: Option<&str>, detail: Option<&str>| CaptchaWatchDetection {
            signal: signal.to_string(),
            name: name.map(str::to_string),
            kind: kind.map(str::to_string),
            challenge: false,
            hidden: false,
            detail: detail.map(str::to_string),
        };
        let state = |s: &str, name: Option<&str>, how: Option<&str>| CaptchaWatchStateReading {
            state: s.to_string(),
            name: name.map(str::to_string),
            how: how.map(str::to_string),
        };

        // 1. One-line description matches `test_a_finding_is_told_in_one_line`.
        assert_eq!(
            describe_detections(&[
                det(
                    "frame",
                    Some("reCAPTCHA"),
                    Some("image"),
                    Some("https://www.google.com/recaptcha/api2/bframe?hl=en"),
                ),
                det(
                    "script",
                    Some("reCAPTCHA"),
                    Some("score"),
                    Some("https://www.google.com/recaptcha/api2/api.js?render=6Lcx"),
                ),
                det("script", Some("Kasada"), Some("sensor"), Some("/ips.js")),
                det("widget", None, None, Some("2")),
                det("token", None, None, Some("cf-turnstile-response")),
                det("honeypot", None, None, Some("website")),
            ]),
            "reCAPTCHA (image, score); Kasada (sensor); 2 page widgets; token field cf-turnstile-response; honeypot field website"
        );
        assert_eq!(
            describe_detections(&[det("widget", None, None, Some("1"))]),
            "1 page widget"
        );
        assert_eq!(describe_detections(&[]), "");

        // 2. Signature is order-independent.
        let pair = vec![
            det("frame", Some("hCaptcha"), Some("image"), Some("a")),
            det("token", None, None, Some("h-captcha-response")),
        ];
        let mut rev = pair.clone();
        rev.reverse();
        assert_eq!(signature_of_detections(&pair), signature_of_detections(&rev));
        assert_ne!(signature_of_detections(&pair), signature_of_detections(&pair[1..]));

        // 3. Per-tab dedupe, subframe merge, strongest state, live transitions, and delayed replacement.
        let mut watch = CaptchaWatchService::default();
        let tab = TabId(1);
        let (r1, changed1) = watch.report(
            tab,
            "https://shop.example/checkout",
            &[det("widget", None, None, Some("1"))],
            &state("none", None, None),
            0,
            true,
            1000,
        );
        assert!(changed1);
        assert_eq!(r1.as_ref().unwrap().text, "1 page widget");

        let (r2, changed2) = watch.report(
            tab,
            "https://verify.example/px",
            &[det("frame", Some("HUMAN"), Some("behavioral"), Some("d"))],
            &state("unchecked", Some("HUMAN"), Some("checkbox is not ticked")),
            7,
            false,
            1100,
        );
        assert!(changed2);
        let r2 = r2.unwrap();
        assert_eq!(r2.url, "https://shop.example/checkout");
        assert_eq!(r2.state, "unchecked");
        assert!(r2.text.contains("HUMAN"));

        // Subframe holding a state stays for REPLACE_WAIT_MS when dropped while top stays.
        let (r_deferred, changed_def) = watch.drop_frame(tab, 7, 1500);
        assert!(!changed_def);
        assert_eq!(r_deferred.unwrap().state, "unchecked");

        // After REPLACE_WAIT_MS expires, subframe is dropped.
        let expired = watch.sweep_expired_drops(1500 + REPLACE_WAIT_MS);
        assert_eq!(expired.len(), 1);
        assert_eq!(watch.last_for(tab).unwrap().state, "none");
        assert!(!watch.last_for(tab).unwrap().text.contains("HUMAN"));

        // Sanitization drops unknown signals and clamps `how` to 160 chars.
        let tab2 = TabId(2);
        let (san, _) = watch.report(
            tab2,
            "https://x",
            &[
                det("read-all-cookies", Some("nope"), None, None),
                det("frame", Some("hCaptcha"), Some("image"), Some("d")),
            ],
            &state("pwned", None, Some(&"x".repeat(500))),
            0,
            true,
            2000,
        );
        let san = san.unwrap();
        assert_eq!(san.detections.len(), 1);
        assert_eq!(san.state, "none");

        let tab3 = TabId(3);
        let (state_only, _) = watch.report(
            tab3,
            "https://x",
            &[],
            &state("solved", Some("Turnstile"), Some(&"y".repeat(500))),
            0,
            true,
            2100,
        );
        let state_only = state_only.unwrap();
        assert_eq!(state_only.state, "solved");
        assert!(state_only.how.unwrap().chars().count() <= 160);
    }
}
