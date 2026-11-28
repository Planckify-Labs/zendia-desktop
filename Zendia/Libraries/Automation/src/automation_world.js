/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

// The Automation World's runtime. It runs in an isolated world of the page, so the page's scripts can neither see nor
// replace it. The browser core asks it to find, read and act on the page, one JSON request at a time
// (`__zendiaAutomation`), and gets one JSON answer back. It draws nothing: the browser draws labels and the visual
// mode cursor over the page from what it answers.
//
// Its behaviour follows the old Zendia's vim mode: which elements are controls, where `j` scrolls, how jump targets
// are matched and ordered, how visual mode's motions and text objects move the selection, and how fields are chosen.
(function zendiaAutomationInstall() {
  "use strict";

  if (typeof globalThis.__zendiaAutomation === "function") {
    return;
  }

  const RUNTIME_SOURCE = "(" + zendiaAutomationInstall.toString() + ")();";

  // ---------------------------------------------------------------------------------------------------------------
  // The DOM, across open shadow roots
  // ---------------------------------------------------------------------------------------------------------------

  const NOT_TEXT_TAGS = new Set([
    "script",
    "style",
    "noscript",
    "textarea",
    "select",
    "datalist",
    "option",
    "input",
    "template",
    "head",
    "defs",
    "symbol",
    "clippath",
    "mask",
    "pattern",
    "filter",
    "lineargradient",
    "radialgradient",
    "metadata",
    "title",
    "desc",
  ]);
  const NON_TEXT_INPUTS = new Set([
    "button",
    "checkbox",
    "color",
    "file",
    "hidden",
    "image",
    "radio",
    "range",
    "reset",
    "submit",
  ]);

  function isNotTextElement(node) {
    if (!node || node.nodeType !== 1) return false;
    return NOT_TEXT_TAGS.has((node.localName || node.tagName || "").toLowerCase());
  }

  function shadowRootOf(node) {
    if (!node || node.nodeType !== 1) return null;
    if (typeof globalThis.__zendiaShadowRoot === "function") {
      try {
        const root = globalThis.__zendiaShadowRoot(node);
        if (root) return root;
      } catch (_) {}
    }
    return node.shadowRoot || null;
  }

  function postEvent(payload) {
    if (typeof globalThis.__zendiaPost === "function") {
      try {
        globalThis.__zendiaPost(JSON.stringify(payload));
      } catch (_) {}
    }
  }

  function composedParent(node) {
    if (!node) return null;
    if (node.parentElement) return node.parentElement;
    const root = node.getRootNode ? node.getRootNode() : null;
    return root && root.host ? root.host : null;
  }

  function composedContains(ancestor, element) {
    for (let each = element; each; each = composedParent(each)) {
      if (each === ancestor) return true;
    }
    return false;
  }

  function deepActiveElement() {
    let element = document.activeElement;
    let shadow = shadowRootOf(element);
    while (shadow && shadow.activeElement) {
      element = shadow.activeElement;
      shadow = shadowRootOf(element);
    }
    return element;
  }

  function* deepElements(root) {
    if (!root || !root.querySelectorAll) return;
    for (const element of root.querySelectorAll("*")) {
      yield element;
      const shadow = shadowRootOf(element);
      if (shadow) yield* deepElements(shadow);
    }
  }

  // Text nodes in document order within a single DOM root (without crossing into child shadow roots).
  function* rootTextNodes(root) {
    if (!root) return;
    const stack = [root];
    while (stack.length) {
      const node = stack.pop();
      if (node.nodeType === 3) {
        yield node;
        continue;
      }
      if (node.nodeType === 1 && isNotTextElement(node)) continue;
      const children = node.childNodes;
      for (let i = children.length - 1; i >= 0; i--) stack.push(children[i]);
    }
  }

  // Text nodes in document order, those of a shadow root before its host's own children, skipping non-text subtrees.
  function* deepTextNodes(root) {
    if (!root) return;
    const stack = [root];
    while (stack.length) {
      const node = stack.pop();
      if (node.nodeType === 3) {
        yield node;
        continue;
      }
      if (node.nodeType === 1 && isNotTextElement(node)) continue;
      const children = [];
      const shadow = shadowRootOf(node);
      if (shadow) children.push(...shadow.childNodes);
      children.push(...node.childNodes);
      for (let i = children.length - 1; i >= 0; i--) stack.push(children[i]);
    }
  }

  function deepElementFromPoint(x, y) {
    let element = document.elementFromPoint(x, y);
    let shadow = shadowRootOf(element);
    while (shadow && typeof shadow.elementFromPoint === "function") {
      const inner = shadow.elementFromPoint(x, y);
      if (!inner || inner === element) break;
      element = inner;
      shadow = shadowRootOf(element);
    }
    return element;
  }

  // Elements in document order, a shadow root's before its host's own children, outer before inner.
  function sortComposed(elements) {
    const paths = new Map(
      elements.map(element => {
        const path = [];
        for (let each = element; each; each = composedParent(each)) path.push(each);
        return [element, path.reverse()];
      })
    );
    return [...elements].sort((a, b) => {
      const first = paths.get(a);
      const second = paths.get(b);
      let depth = 0;
      while (depth < first.length && depth < second.length && first[depth] === second[depth]) depth++;
      if (depth === first.length || depth === second.length) return first.length - second.length;
      const x = first[depth];
      const y = second[depth];
      const inShadow = element => !!(element.getRootNode && element.getRootNode().host);
      if (inShadow(x) !== inShadow(y)) return inShadow(x) ? -1 : 1;
      return x.compareDocumentPosition(y) & 4 ? -1 : 1;
    });
  }

  // Whether typing into an element edits it, which puts the page in insert mode.
  function isEditable(element) {
    if (!element || element.nodeType !== 1) return false;
    if (element.isContentEditable || document.designMode === "on") return true;
    switch (element.localName) {
      case "textarea":
        return !element.readOnly && !element.disabled;
      case "select":
        return !element.disabled;
      case "input":
        return !NON_TEXT_INPUTS.has((element.type || "text").toLowerCase()) && !element.readOnly && !element.disabled;
    }
    return false;
  }

  // Whether a control is rendered, matching the fork's `element.checkVisibility()` with no options (`JMP-02`):
  // `opacity: 0` controls (styled checkboxes, radios, file inputs) remain eligible when paired with `shownPoint`.
  function isRendered(element) {
    if (!element || !element.isConnected) return false;
    if (typeof element.checkVisibility === "function") {
      try {
        return element.checkVisibility();
      } catch (_) {}
    }
    for (let each = element; each && each.nodeType === 1; each = composedParent(each)) {
      const style = getComputedStyle(each);
      if (!style || style.display === "none" || style.contentVisibility === "hidden") return false;
    }
    return element.getClientRects().length > 0;
  }

  const ZERO_CLIP_PATH = /^inset\(\s*(?:50%|100%)|^circle\(\s*0(?![.\d])|^polygon\(\s*0(?:\s|px|%)+\s*0(?:\s|px|%)+\s*,\s*0(?:\s|px|%)+\s*0(?:\s|px|%)+\s*(?:,\s*0(?:\s|px|%)+\s*0(?:\s|px|%)+\s*)*\)/i;

  function isTextAncestorBoxRendered(element, ancCache) {
    if (!element || element.nodeType !== 1) return true;
    if (!element.isConnected) return false;
    if (ancCache && ancCache.has(element)) return ancCache.get(element);
    const style = getComputedStyle(element);
    let ok =
      !!style &&
      style.display !== "none" &&
      style.contentVisibility !== "hidden" &&
      !(parseFloat(style.opacity) < 0.05) &&
      !ZERO_CLIP_PATH.test(style.clipPath || "");
    if (ok) {
      const parent = composedParent(element);
      if (parent && parent.nodeType === 1) ok = isTextAncestorBoxRendered(parent, ancCache);
    }
    if (ancCache) ancCache.set(element, ok);
    return ok;
  }

  // Whether an element's text is visibly rendered on the page (excluding `visibility: hidden`/`collapse`, `opacity: 0`
  // dropdowns/tooltips, `display: none`, `content-visibility: hidden`, and `font-size: 0`, while supporting `display: contents`).
  function isTextRendered(element, cache, ancCache) {
    if (!element || element.nodeType !== 1 || !element.isConnected || isNotTextElement(element)) return false;
    if (cache && cache.has(element)) return cache.get(element);
    const style = getComputedStyle(element);
    let ok =
      !!style &&
      style.visibility !== "hidden" &&
      style.visibility !== "collapse" &&
      (style.display === "contents" || parseFloat(style.fontSize) !== 0) &&
      isTextAncestorBoxRendered(element, ancCache);
    if (ok && style.display !== "contents" && typeof element.checkVisibility === "function") {
      try {
        ok = element.checkVisibility({ opacityProperty: true, visibilityProperty: true });
      } catch (_) {}
    }
    if (cache) cache.set(element, ok);
    return ok;
  }

  // Child frame discovery, coordinate mapping, and isolated-world evaluation (`FRM-01`).
  const frameIds = new WeakMap();
  let nextFrameId = 0;

  function frameId(frame) {
    let id = frameIds.get(frame);
    if (!id) {
      id = "f" + nextFrameId++;
      frameIds.set(frame, id);
    }
    return id;
  }

  function docInfo() {
    if (typeof globalThis.__zendiaDocInfo === "function") {
      try {
        const raw = globalThis.__zendiaDocInfo();
        if (typeof raw === "string") return JSON.parse(raw);
      } catch (_) {}
    }
    return {
      frameId: window.top === window ? 0 : 1,
      top: window.top === window,
      url: String(document.documentURI || location.href || ""),
    };
  }

  function frameInfoOf(frame) {
    if (typeof globalThis.__zendiaFrameInfo === "function" && frame && frame.nodeType === 1) {
      try {
        const raw = globalThis.__zendiaFrameInfo(frame);
        if (typeof raw === "string") return JSON.parse(raw);
      } catch (_) {}
    }
    return null;
  }

  async function evalInFrame(frame, requestObj) {
    if (typeof globalThis.__zendiaEvalFrame !== "function" || !frame || frame.nodeType !== 1) return null;
    const payload = JSON.stringify(JSON.stringify(requestObj));
    try {
      let raw = await globalThis.__zendiaEvalFrame(
        frame,
        "typeof __zendiaAutomation === 'function' ? __zendiaAutomation(" + payload + ") : '__NEED_RUNTIME__'"
      );
      if (raw === "__NEED_RUNTIME__") {
        raw = await globalThis.__zendiaEvalFrame(frame, RUNTIME_SOURCE + "\n__zendiaAutomation(" + payload + ")");
      }
      return typeof raw === "string" ? JSON.parse(raw) : null;
    } catch (_) {
      return null;
    }
  }

  function frameVisible(frame) {
    if (!frame || !frame.isConnected) return false;
    if (typeof frame.checkVisibility === "function") {
      try {
        if (!frame.checkVisibility({ visibilityProperty: true })) return false;
      } catch (_) {
        try {
          if (!frame.checkVisibility()) return false;
        } catch (_) {}
      }
    } else {
      const st = getComputedStyle(frame);
      if (!st || st.display === "none" || st.visibility === "hidden" || st.visibility === "collapse") return false;
    }
    return getComputedStyle(frame).pointerEvents !== "none";
  }

  function allFrames() {
    const root = document.body || document.documentElement;
    if (!root) return [];
    const found = [];
    for (const el of deepElements(root)) {
      if (el.localName === "iframe" || el.localName === "frame") found.push(el);
    }
    return sortComposed(found);
  }

  function visibleFrames() {
    return allFrames().filter(frame => frameVisible(frame) && isInWindow(frame.getBoundingClientRect()));
  }

  const initializedFrames = new WeakMap();

  async function initChildFrames(forceAll) {
    const frames = allFrames().filter(frame => {
      if (forceAll) return true;
      const src = frameSrcOf(frame);
      const prev = initializedFrames.get(frame);
      return prev === undefined || (src && prev !== src);
    });
    if (!frames.length) return;
    await Promise.all(
      frames.map(async frame => {
        const src = frameSrcOf(frame);
        const res = await evalInFrame(frame, { op: "init" });
        if (res && res.kind === "ok") initializedFrames.set(frame, src || "");
      })
    );
  }

  function frameBox(frame) {
    const box = frame.getBoundingClientRect();
    const scale = (drawn, laid) => {
      const value = laid ? drawn / laid : 1;
      return Number.isFinite(value) && value > 0 ? value : 1;
    };
    const scaleX = scale(box.width, frame.offsetWidth);
    const scaleY = scale(box.height, frame.offsetHeight);
    const left = box.left + frame.clientLeft * scaleX;
    const top = box.top + frame.clientTop * scaleY;
    const width = frame.clientWidth * scaleX;
    const height = frame.clientHeight * scaleY;
    return { left, top, width, height, right: left + width, bottom: top + height, scaleX, scaleY };
  }

  function fromFrame(box, rect) {
    const rx = rect.left !== undefined ? rect.left : rect.x;
    const ry = rect.top !== undefined ? rect.top : rect.y;
    const left = box.left + rx * box.scaleX;
    const top = box.top + ry * box.scaleY;
    const width = rect.width * box.scaleX;
    const height = rect.height * box.scaleY;
    return { left, top, width, height, right: left + width, bottom: top + height, x: left, y: top };
  }

  function frameShowsAt(frame, x, y) {
    if (x < 0 || y < 0 || x >= innerWidth || y >= innerHeight) return true;
    const hit = deepElementFromPoint(x, y);
    return !hit || hit === frame || composedContains(frame, hit);
  }

  function uncoveredInFrame(frame, box, mappedRect) {
    const cx = mappedRect.left + mappedRect.width / 2;
    const cy = mappedRect.top + mappedRect.height / 2;
    return cx >= box.left && cx <= box.right && cy >= box.top && cy <= box.bottom && frameShowsAt(frame, cx, cy);
  }

  function plainRect(rect) {
    return { x: rect.left, y: rect.top, width: rect.width, height: rect.height };
  }

  function onScreen(rect) {
    return (
      rect.width > 0 &&
      rect.height > 0 &&
      rect.bottom > 0 &&
      rect.right > 0 &&
      rect.top < innerHeight &&
      rect.left < innerWidth
    );
  }

  function center() {
    return { x: innerWidth / 2, y: innerHeight / 2 };
  }

  function distance(rect, from) {
    return Math.hypot(rect.left + rect.width / 2 - from.x, rect.top + rect.height / 2 - from.y);
  }

  // ---------------------------------------------------------------------------------------------------------------
  // Controls & CAPTCHA recognition (`ZendiaControls.sys.mjs`, `ZendiaCaptcha.sys.mjs`, `ZendiaPointerContent.sys.mjs`, `CAP-01`..`CAP-09`)
  // ---------------------------------------------------------------------------------------------------------------

  const CONTROLS = [
    "checkbox",
    "radio",
    "menulist",
    "richlistitem",
    "tab",
    "toolbarbutton",
    "a[href]",
    "button",
    "input:not([type=hidden])",
    "select",
    "textarea",
    "summary",
    "[role=button]",
    "[role=link]",
    "[role=checkbox]",
    "[role=switch]",
    "[role=tab]",
    "[role=menuitem]",
    "[role=radio]",
    "[role=option]",
    "[role=combobox]",
    "[role=textbox]",
    "[role=slider]",
    "[contenteditable]:not([contenteditable=false])",
    "[tabindex]:not([tabindex='-1'])",
    "[onclick]",
  ].join(",");
  const FIELDS = "input, textarea, select, [contenteditable='true'], [contenteditable='']";
  const DRAG_CURSORS = new Set([
    "grab",
    "grabbing",
    "move",
    "all-scroll",
    "ew-resize",
    "e-resize",
    "w-resize",
    "col-resize",
  ]);
  const HANDS = new Set(["pointer", ...DRAG_CURSORS]);
  const DRAG_NAME = /slider|handle|knob|thumb|drag|puzzle|jigsaw|press-?(?:and-?)?hold/i;
  const HOLD_WORDS = /\bpress\s*(?:&|and)\s*hold\b|\b(?:click|tap) and hold\b|\bhold (?:the|this) button\b/i;
  const HOLD_TEXT_MAX = 80;
  const SLIDER_NAME = /slid|knob|thumb/i;
  const MIN_SIZE = 4;
  const FRAGMENTS_MAX = 4;
  // Where in a box to look for the control: the middle first, then around it.
  const SPOTS = [
    [0.5, 0.5],
    [0.25, 0.5],
    [0.75, 0.5],
    [0.5, 0.25],
    [0.5, 0.75],
  ];

  const CAPTCHAS = [
    {
      name: "reCAPTCHA",
      frame: /google\.com\/recaptcha\/|recaptcha\.net\//,
      challenge: /\/bframe/,
      kind: "image",
      widgetKind: "checkbox",
      score: /[?&]render=(?!(?:explicit|onload)\b)/,
    },
    {
      name: "hCaptcha",
      frame: /hcaptcha\.com\//,
      challenge: /frame=challenge/,
      kind: "image",
      widgetKind: "checkbox",
    },
    {
      name: "Cloudflare Turnstile",
      frame: /challenges\.cloudflare\.com\/|\/cdn-cgi\/challenge-platform\//,
      challenge: null,
      kind: "managed",
    },
    { name: "Arkose", frame: /arkoselabs\.com\/|funcaptcha\.com\/|octocaptcha\.com\//, challenge: /./, kind: "game" },
    { name: "GeeTest", frame: /geetest\.com\/|geevisit\.com\/|geetest\.cn\//, challenge: /./, kind: "slider" },
    {
      name: "HUMAN",
      frame: /px-cdn\.net\/|px-cloud\.net\/|perimeterx\.(?:net|com)\/|human\.com\/captcha/,
      challenge: /./,
      kind: "behavioral",
    },
    { name: "DataDome", frame: /captcha-delivery\.com\//, challenge: /./, kind: "slider" },
    {
      name: "Yandex SmartCaptcha",
      frame: /smartcaptcha\.yandexcloud\.net\/|captcha\.yandex\./,
      challenge: /advanced/,
      kind: "puzzle",
      widgetKind: "checkbox",
    },
    { name: "Tencent Captcha", frame: /captcha\.qcloud\.com\/|turing\.captcha\.gtimg\.com\//, challenge: /./, kind: "slider" },
    { name: "NetEase Yidun", frame: /(?:c|captcha)\.dun\.163\.com\//, challenge: /./, kind: "slider" },
    { name: "MTCaptcha", frame: /mtcaptcha\.com\//, challenge: null, kind: "text" },
    {
      name: "Friendly Captcha",
      frame: /\bfrcapi\.com\/|\b(?:eu-)?api\.friendlycaptcha\.(?:com|eu)\//,
      challenge: null,
      kind: "proof-of-work",
    },
    { name: "AWS WAF", frame: /awswaf\.com\//, challenge: /./, kind: "puzzle" },
    { name: "Lemin", frame: /leminnow\.com\//, challenge: /./, kind: "puzzle" },
    { name: "KeyCAPTCHA", frame: /keycaptcha\.com\//, challenge: /./, kind: "puzzle" },
  ];

  const WIDGETS = [
    ".g-recaptcha",
    ".grecaptcha-badge",
    ".h-captcha",
    ".cf-turnstile",
    ".frc-captcha",
    "#lemin-cropped-captcha",
    ".lemin-captcha-popup",
    "altcha-widget",
    ".altcha",
    ".mtcaptcha",
    "#mtcaptcha",
    ".mcaptcha__widget-container",
    "[class*='geetest_']",
    ".gt_holder",
    ".gt_popup",
    "[class*='yidun']",
    "[id*='tcaptcha']",
    "form[action*='validateCaptcha']",
    "#px-captcha",
    "#cf-challenge-running",
    "#challenge-stage",
    "#challenge-form",
    "#amzn-captcha-verify-button",
    ".amzn-captcha-modal",
    "awswaf-captcha",
  ].join(",");

  const NOT_OPENED = [
    ".frc-captcha",
    "altcha-widget",
    ".altcha",
    ".mcaptcha__widget-container",
    ".cf-turnstile",
    "#cf-challenge-running",
    "#challenge-stage",
    "#challenge-form",
    ".mtcaptcha",
    "#mtcaptcha",
    ".g-recaptcha",
    ".grecaptcha-badge",
    ".h-captcha",
  ].join(",");

  const SCRIPTS = [
    {
      name: "reCAPTCHA",
      script: /recaptcha\/[^?]*\.js\?[^#]*\brender=(?!(?:explicit|onload)\b)/,
      kind: "score",
    },
    {
      name: "reCAPTCHA",
      script: /google\.com\/recaptcha\/|recaptcha\.net\/|gstatic\.com\/recaptcha\//,
      kind: "checkbox",
    },
    {
      name: "hCaptcha",
      script: /(?:js\.)?hcaptcha\.com\/1\/api\.js/,
      kind: "checkbox",
    },
    {
      name: "Cloudflare Turnstile",
      script: /challenges\.cloudflare\.com\/turnstile\//,
      kind: "managed",
    },
    {
      name: "Arkose",
      script: /arkoselabs\.com\/|funcaptcha\.com\//,
      kind: "game",
    },
    {
      name: "GeeTest",
      script: /geetest\.(?:com|cn)\/|geevisit\.com\/|\/gt4[\w.-]*\.js(?:[?#]|$)/,
      kind: "slider",
    },
    {
      name: "DataDome",
      script: /js\.datadome\.co\/tags\.js/,
      kind: "behavioral",
    },
    {
      name: "HUMAN",
      script: /px-cdn\.net\/|px-cloud\.net\//,
      kind: "behavioral",
    },
    { name: "AWS WAF", script: /awswaf\.com\//, kind: "puzzle" },
    {
      name: "Friendly Captcha",
      script: /@friendlycaptcha\/sdk|friendly-challenge|\bfrcapi\.com\/|\b(?:eu-)?api\.friendlycaptcha\.(?:com|eu)\//,
      kind: "proof-of-work",
    },
    { name: "Lemin", script: /leminnow\.com\//, kind: "puzzle" },
    { name: "ALTCHA", script: /\/altcha[@/.]/, kind: "proof-of-work" },
    { name: "MTCaptcha", script: /mtcaptcha\.com\//, kind: "text" },
    {
      name: "Yandex SmartCaptcha",
      script: /smartcaptcha\.yandexcloud\.net\/|captcha\.yandex\./,
      kind: "puzzle",
    },
    { name: "Kasada", script: /\/ips\.js(?:[?#]|$)/, kind: "sensor" },
  ];

  const TOKEN_FIELDS = [
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
  const TOKEN_PLACEHOLDER = /^\.([A-Z][A-Z_]*)$/;
  const PLACEHOLDER_FAILED = /(?:^|_)(?:EXPIRED|ERROR|TIMEOUT|RESET|DESTROYED|FAILED)$/;
  const PLACEHOLDER_RUNNING = /^(?:FETCHING|UNFINISHED|SOLVING|REQUESTING|VERIFYING|ACTIVATED)$/;
  const NOT_A_TOKEN = /^(?:null|undefined|false|0)$/i;
  const HONEYPOT_NAMES =
    /^(?:hp|honeypot|honey|sweet|website|homepage|url|link|email2|e-?mail2|fax|company|leave_?blank|do_?not_?fill|keep_?empty|comment_?hp)$/i;
  const STATES = ["open", "unchecked", "failed", "shown", "checking", "solved", "none"];

  const OWN_CAPTCHA_NAMES =
    /captcha|security[\s_-]?code|verification[\s_-]?code|verify[\s_-]?code|anti[\s_-]?bot|human[\s_-]?check|enter (?:the )?(?:characters|code|text|letters|numbers) (?:you see|shown|above|below|in the (?:image|picture))/i;
  const NOT_OWN_CAPTCHA = /logo|icon|badge|recaptcha-response|captcha-response/i;
  const SERVICE_SITES =
    /^https?:\/\/www\.(?:hcaptcha|arkoselabs|geetest|mtcaptcha|friendlycaptcha|leminnow|keycaptcha)\.(?:com|cn|eu)\//;

  const CHALLENGE_PROMPTS = [
    [
      "image",
      /^(?:please )?(?:select|click|tap|pick)(?: on)?(?: all| each| every| any)?(?: of)?(?: the)? (?:images?|squares?|pictures?|tiles?|photos?|boxes)\b/i,
    ],
    [
      "text",
      /^(?:please )?(?:type|enter)(?: in)?(?: the)? (?:characters?|letters?|text|code|words?|numbers?)\b.*\b(?:see|shown|image|picture|above|below)\b/i,
    ],
    [
      "slider",
      /^(?:please )?(?:slide|drag)\b.*\b(?:into place|gap|to (?:complete|fit|verify)|right)\b/i,
    ],
    ["rotate", /^(?:please )?(?:rotate|turn) the (?:image|picture|object)\b/i],
    [
      "click",
      /^(?:please )?(?:click|tap|select|pick)(?: on)?(?: the)?(?: (?:icons?|characters?|symbols?|letters?|objects?|images?))? (?:in|with) (?:the |this )?(?:following |given )?order\b/i,
    ],
    ["math", /^(?:what is|solve|calculate|how much is) \d+ ?[-+x×*÷/] ?\d+/i],
    ["behavioral", /^press (?:&|and) hold\b/i],
    [
      "checkbox",
      /^(?:i['’]?m|i am) (?:not a robot|a human|human)\.?$|^verify (?:that )?you(?:['’]re| are)(?: a)? human\.?$|^(?:are you a )?robot or human\??$|^click (?:here )?to verify\.?$/i,
    ],
  ];
  const PROMPT_WORDS = [
    "select",
    "click",
    "tap",
    "pick",
    "type",
    "enter",
    "slide",
    "drag",
    "rotate",
    "turn",
    "robot",
    "human",
    "press",
    "what is",
    "solve",
    "calculate",
    "how much",
  ];
  const PROMPT_MAX = 120;

  const PURPOSES = ["answer", "play", "visual", "audio", "reload", "verify"];
  const NOT_A_PURPOSE = /download|help|privacy|terms|report|feedback|logo/i;
  const PURPOSE_RULES = [
    ["play", /\bplay\b|\breplay\b|listen again|play-?button/i],
    ["visual", /\bvisual\b|image-?button|get an? (?:image|picture)|(?:image|picture) challenge instead/i],
    ["audio", /\baudio\b|\blisten\b|headphone|\bsound\b|\bvoice\b/i],
    [
      "reload",
      /reload|refresh|new challenge|another challenge|try another|try again|start over|restart|^reset\b|new (?:image|picture|puzzle|question)s?\b/i,
    ],
    [
      "verify",
      /\bverify\b|\bsubmit\b|^next\b|\bskip\b|^check\b|\bconfirm\b|^done\b|^continue\b|^ok\b|verify-?button|button-?submit/i,
    ],
  ];
  const ANSWER_TYPES = new Set(["", "text", "search", "tel", "number", "email", "url"]);
  const SOLVED_CLASS = /(?:^|[_-])(?:success|verified|solved|passed)$/i;
  const SOLVED_MARKS = `:is(${WIDGETS}):is([class*=success], [class*=verified], [class*=solved], [class*=passed], [data-state=verified])`;

  function serviceOfUrl(url) {
    const s = String(url || "");
    return SERVICE_SITES.test(s) ? null : CAPTCHAS.find(c => c.frame.test(s)) || null;
  }

  function loaderOfUrl(url) {
    const s = String(url || "");
    return SERVICE_SITES.test(s) ? null : SCRIPTS.find(c => c.script.test(s)) || null;
  }

  function nameOfUrl(url) {
    const svc = serviceOfUrl(url);
    return svc ? svc.name : null;
  }

  function isChallengeUrl(url) {
    const svc = serviceOfUrl(url);
    return !!(svc && svc.challenge && svc.challenge.test(String(url || "")));
  }

  function nameOfScriptUrl(url) {
    const loader = loaderOfUrl(url);
    return loader ? loader.name : null;
  }

  function kindOfScriptUrl(url) {
    const loader = loaderOfUrl(url);
    return loader ? loader.kind : null;
  }

  function kindOfUrl(url) {
    const s = String(url || "");
    const captcha = serviceOfUrl(s);
    if (!captcha) return kindOfScriptUrl(s);
    if (captcha.score && captcha.score.test(s)) return "score";
    if (!captcha.challenge) return captcha.kind;
    return captcha.challenge.test(s) ? captcha.kind : captcha.widgetKind || captcha.kind;
  }

  function strongerState(a, b) {
    const rank = state => {
      const index = STATES.indexOf(state);
      return index < 0 ? STATES.length : index;
    };
    return rank(b) < rank(a) ? b : a;
  }

  function tokenStateOf(value) {
    const token = String(value == null ? "" : value).trim();
    if (!token || NOT_A_TOKEN.test(token)) return null;
    const match = TOKEN_PLACEHOLDER.exec(token);
    const placeholder = match ? match[1] : null;
    if (!placeholder) return "solved";
    if (PLACEHOLDER_FAILED.test(placeholder)) return "failed";
    return PLACEHOLDER_RUNNING.test(placeholder) ? "checking" : null;
  }

  function isHoneypotField(name, visible) {
    return !visible && HONEYPOT_NAMES.test(String(name || ""));
  }

  function frameSrcOf(frame) {
    if (!frame) return "";
    const attr = frame.src || frame.getAttribute("src") || "";
    if (attr && !/^about:blank(?:$|[?#])/i.test(attr)) return attr;
    const info = frameInfoOf(frame);
    if (info && info.url) return info.url;
    return attr;
  }

  function promptKind(text) {
    const s = String(text || "").replace(/\s+/g, " ").trim();
    if (!s || s.length > PROMPT_MAX) return null;
    const hit = CHALLENGE_PROMPTS.find(entry => entry[1].test(s));
    return hit ? hit[0] : null;
  }

  function isTokenField(name) {
    const field = String(name || "").toLowerCase();
    return TOKEN_FIELDS.some(
      token => field === token || field.startsWith(token + "-") || field.startsWith(token + "[")
    );
  }

  function marksSolved(classes, state) {
    if (state === "verified") return true;
    for (const name of classes || []) {
      if (SOLVED_CLASS.test(name)) return true;
    }
    return false;
  }

  function namesOwnCaptcha(names) {
    const text = String(names || "").slice(0, 1000);
    return OWN_CAPTCHA_NAMES.test(text) && !NOT_OWN_CAPTCHA.test(text);
  }

  function purposeOfControl(control) {
    const tag = String(control.tag || "").toLowerCase();
    const type = String(control.type || "").toLowerCase();
    if (control.editable && tag !== "select" && (tag !== "input" || ANSWER_TYPES.has(type))) {
      return "answer";
    }
    if (tag === "input" && (type === "checkbox" || type === "radio")) {
      return null;
    }
    const label = String(control.label || "").replace(/\s+/g, " ").trim().slice(0, 200);
    const name = String(control.name || "").replace(/[_\s]+/g, " ").slice(0, 200);
    if (NOT_A_PURPOSE.test(label) || NOT_A_PURPOSE.test(name)) {
      return null;
    }
    for (const text of [label, name]) {
      for (const [purpose, rule] of PURPOSE_RULES) {
        if (text && rule.test(text)) return purpose;
      }
    }
    return null;
  }

  function rawTrackOf(element) {
    const knob = element.getBoundingClientRect();
    if (knob.height < 20 || knob.width < 20) return null;
    let track = composedParent(element);
    for (let level = 0; track && level < 3; level++) {
      const box = track.getBoundingClientRect ? track.getBoundingClientRect() : null;
      if (
        box &&
        box.width >= knob.width * 2.5 &&
        box.width <= 640 &&
        box.height >= knob.height * 0.45 &&
        box.height <= Math.max(knob.height * 1.8, 72) &&
        (Math.abs(knob.left - box.left) < 16 || Math.abs(knob.right - box.right) < 16) &&
        ![...track.querySelectorAll(CONTROLS)].some(
          other => other !== element && !element.contains(other) && !other.contains(element)
        )
      ) {
        return box;
      }
      track = composedParent(track);
    }
    return null;
  }

  function sitsInATrack(element) {
    return !!rawTrackOf(element);
  }

  function isDraggable(element) {
    return (
      DRAG_CURSORS.has(getComputedStyle(element).cursor) ||
      element.matches("[role=slider], [draggable=true]") ||
      DRAG_NAME.test((element.getAttribute("class") || "") + " " + (element.id || "")) ||
      HOLD_WORDS.test((element.getAttribute("aria-label") || "") + " " + (element.textContent || "").slice(0, HOLD_TEXT_MAX)) ||
      sitsInATrack(element)
    );
  }

  function isSlider(element) {
    return (
      element.matches("[role=slider], input[type=range]") ||
      SLIDER_NAME.test((element.getAttribute("class") || "") + " " + (element.id || "")) ||
      sitsInATrack(element)
    );
  }

  function trackOf(element) {
    const direct = rawTrackOf(element);
    if (direct) {
      return { x: direct.left, y: direct.top, left: direct.left, top: direct.top, width: direct.width, height: direct.height };
    }
    const knob = element.getBoundingClientRect();
    let parent = composedParent(element);
    let fallback = null;
    for (let level = 0; parent && level < 4; level++) {
      const box = parent.getBoundingClientRect ? parent.getBoundingClientRect() : null;
      if (box) {
        if (!fallback) fallback = box;
        if (box.width >= Math.max(knob.width * 2.5, 140) && box.width <= 640 && box.height <= Math.max(knob.height * 2.5, 90)) {
          return { x: box.left, y: box.top, left: box.left, top: box.top, width: box.width, height: box.height };
        }
      }
      parent = composedParent(parent);
    }
    if (!fallback) return null;
    return { x: fallback.left, y: fallback.top, left: fallback.left, top: fallback.top, width: fallback.width, height: fallback.height };
  }

  function isInWindow(rect) {
    return (
      rect.width >= MIN_SIZE &&
      rect.height >= MIN_SIZE &&
      rect.bottom > 0 &&
      rect.right > 0 &&
      rect.top < innerHeight &&
      rect.left < innerWidth
    );
  }

  function isOnTopAt(element, x, y) {
    const hit = deepElementFromPoint(x, y);
    if (!hit) return false;
    if (composedContains(element, hit) || composedContains(hit, element)) return true;
    const label = hit.closest ? hit.closest("label") : null;
    return !!label && label.control === element;
  }

  // A point in the window where the control is on top, or null when something covers all of it. A link that wraps
  // is looked for in each of its line boxes too.
  function shownPoint(element, rect) {
    const boxes = [rect];
    const fragments = element.getClientRects();
    if (fragments.length > 1) boxes.push(...[...fragments].slice(0, FRAGMENTS_MAX));
    for (const box of boxes) {
      const left = Math.max(box.left, 0);
      const top = Math.max(box.top, 0);
      const right = Math.min(box.right, innerWidth);
      const bottom = Math.min(box.bottom, innerHeight);
      if (right - left < 1 || bottom - top < 1) continue;
      for (const [across, down] of SPOTS) {
        const x = left + (right - left) * across;
        const y = top + (bottom - top) * down;
        if (isOnTopAt(element, x, y)) return { x, y };
      }
    }
    return null;
  }

  function isField(element) {
    return element.matches(FIELDS) && isEditable(element);
  }

  // The controls on screen, in document order. A hand cursor marks a control that has no markup to say so; a hand
  // inside a control or another hand is not another one.
  function findControls() {
    const cursors = new Map();
    const cursorOf = element => {
      let cursor = cursors.get(element);
      if (cursor === undefined) {
        cursor = getComputedStyle(element).cursor;
        cursors.set(element, cursor);
      }
      return cursor;
    };
    const root = document.body || document.documentElement;
    const shown = [];
    for (const element of deepElements(root)) {
      const rect = element.getBoundingClientRect();
      if (!isInWindow(rect)) continue;
      const isControl = element.matches(CONTROLS);
      let isHand = false;
      if (!isControl) {
        const cursor = cursorOf(element);
        const parent = composedParent(element);
        isHand = HANDS.has(cursor) && !!parent && cursorOf(parent) !== cursor;
      }
      if ((isControl || isHand) && !element.disabled && isRendered(element) && shownPoint(element, rect)) {
        shown.push({ element, isHand });
      }
    }
    const hands = new Set(shown.filter(item => item.isHand).map(item => item.element));
    const found = new Set();
    for (const element of sortComposed(shown.map(item => item.element))) {
      let inside = false;
      if (hands.has(element)) {
        for (let each = composedParent(element); each && !inside; each = composedParent(each)) {
          inside = found.has(each);
        }
      }
      if (!inside) found.add(element);
    }
    return [...found];
  }

  function textOf(element) {
    const image = element.querySelector ? element.querySelector("img[alt]") : null;
    const text =
      element.getAttribute("aria-label") ||
      element.innerText ||
      element.value ||
      element.getAttribute("title") ||
      element.getAttribute("alt") ||
      element.getAttribute("placeholder") ||
      (image && image.getAttribute("alt")) ||
      "";
    return String(text).replace(/\s+/g, " ").trim().slice(0, 200);
  }

  function pointOf(element) {
    const rect = element.getBoundingClientRect();
    return shownPoint(element, rect) || { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  }

  // ---------------------------------------------------------------------------------------------------------------
  // Jump targets
  // ---------------------------------------------------------------------------------------------------------------

  // A pattern with an uppercase letter is matched as typed; any other ignores case.
  function findMatches(text, pattern) {
    if (!pattern) return [];
    const fold = pattern === pattern.toLowerCase();
    const haystack = fold ? text.toLowerCase() : text;
    const matches = [];
    for (let index = haystack.indexOf(pattern); index !== -1; index = haystack.indexOf(pattern, index + 1)) {
      matches.push({ index, next: haystack[index + pattern.length] || "" });
    }
    return matches;
  }

  // What the last scan offered, by the ids the browser picks them with.
  let pickable = new Map();
  // A control keeps its id while it lives, so its label stays with it as a pattern grows.
  const controlIds = new WeakMap();
  let nextControlId = 0;

  function controlId(element) {
    let id = controlIds.get(element);
    if (!id) {
      id = "c" + nextControlId++;
      controlIds.set(element, id);
    }
    return id;
  }

  function target(id, rect, kind, extra) {
    return Object.assign(
      {
        id,
        x: rect.left,
        y: rect.top,
        width: rect.width,
        height: rect.height,
        kind,
        text: "",
        nexts: [],
        distance: 0,
      },
      extra
    );
  }

  // ---------------------------------------------------------------------------------------------------------------
  // Full CAPTCHA detection layer (`ZendiaCaptchaWatchChild.sys.mjs`), drawn challenges, areas & controls (`CAP-01`..`CAP-09`)
  // ---------------------------------------------------------------------------------------------------------------

  const PICTURE_MIN = { width: 100, height: 60 };
  const OWN_CAPTCHA_LEVELS = 4;
  const OWN_CAPTCHA_WIDEN = 4;
  const OWN_PICTURE_MIN = { width: 40, height: 24 };
  const OTHER_FIELDS =
    "input:not([type=hidden], [type=submit], [type=button], [type=reset], [type=image]), textarea, select";
  const DRAWN_MAX = 400;
  const DRAWN_CACHE_MS = 400;
  let drawnChallengesCache = { at: 0, boxes: [] };

  const WATCH_MIN_SIDE = 20;
  const WATCH_PICTURE_SIDE = 36;
  const WATCH_OPEN_SIDE = 260;
  const WATCH_FADED = 0.95;
  const WATCH_PART_ROOM = 4;
  const WATCH_ICON_SIDE = 64;
  const WATCH_TASK_SIDE = 100;
  const WATCH_SMALL_WIDTH = 200;
  const WATCH_SMALL_HEIGHT = 50;
  const WATCH_BOX_MAX_WIDTH = 520;
  const WATCH_BOX_MAX_HEIGHT = 720;
  const WATCH_PROMPT_LEVELS = 6;
  const WATCH_WIDGET_LEVELS = 4;
  const WATCH_WIDGET_GROWTH = 4;
  const WATCH_MOSTLY_VIEW = 0.9;
  const WATCH_LINE_MAX = 80;
  const WATCH_LINE_LEVELS = 2;
  const WATCH_MAX_PROMPT_TEXTS = 200;
  const WATCH_MAX_OWN = 5;
  const WATCH_MAX_INLINE_SCRIPTS = 100;
  const WATCH_MAX_INLINE_TEXT = 50000;
  const WATCH_MAX_INLINE_URLS = 50;
  const WATCH_MAX_EXTENT_ELEMENTS = 2000;
  const WATCH_CONTROLS =
    "img, canvas, svg, video, button, input:not([type=hidden]), select, [role=button], [role=checkbox], [role=slider], [draggable=true], [tabindex]";
  const WATCH_ANSWER_FIELDS =
    "input:not([type]), input[type=text], input[type=tel], input[type=number], input[type=search], textarea";
  const WATCH_NOT_PROMPT = "a[href], h1, h2, option, [hidden], [aria-hidden=true]";
  const WATCH_VISIBILITY = { opacityProperty: true, visibilityProperty: true };

  function checkVisibleStrict(element) {
    if (!element || !element.isConnected) return false;
    if (typeof element.checkVisibility === "function") {
      try {
        return element.checkVisibility(WATCH_VISIBILITY);
      } catch (_) {}
    }
    const st = getComputedStyle(element);
    return !!st && st.display !== "none" && st.visibility !== "hidden" && parseFloat(st.opacity || "1") > 0;
  }

  function isCaptchaFrame() {
    return window.parent !== window && nameOfUrl(document.documentURI || location.href || "") !== null;
  }

  function watchFramesOf() {
    const frames = new Set(document.querySelectorAll("iframe, frame"));
    for (const frame of allFrames()) frames.add(frame);
    return [...frames];
  }

  function watchIsShown(element) {
    if (!checkVisibleStrict(element)) return false;
    const rect = element.getBoundingClientRect();
    return (
      rect.width >= WATCH_MIN_SIDE &&
      rect.height >= WATCH_MIN_SIDE &&
      rect.right + scrollX > 0 &&
      rect.bottom + scrollY > 0
    );
  }

  function watchVisibleRect(element, rect, opts) {
    const box = rect || element.getBoundingClientRect();
    const viewport = !opts || opts.viewport !== false;
    let left = viewport ? Math.max(box.left, 0) : box.left;
    let top = viewport ? Math.max(box.top, 0) : box.top;
    let right = viewport ? Math.min(box.right, innerWidth) : box.right;
    let bottom = viewport ? Math.min(box.bottom, innerHeight) : box.bottom;
    let node = element;
    while (node && right > left && bottom > top) {
      const position = getComputedStyle(node) ? getComputedStyle(node).position : "";
      if (position === "fixed") break;
      const parent = position === "absolute" ? node.offsetParent : node.parentElement;
      if (!parent || parent === document.body || parent === document.documentElement) break;
      const style = getComputedStyle(parent);
      if (style) {
        const pbox = parent.getBoundingClientRect();
        const innerLeft = pbox.left + (parent.clientLeft || 0);
        const innerTop = pbox.top + (parent.clientTop || 0);
        if (style.overflowX !== "visible") {
          left = Math.max(left, innerLeft);
          right = Math.min(right, innerLeft + parent.clientWidth);
        }
        if (style.overflowY !== "visible") {
          top = Math.max(top, innerTop);
          bottom = Math.min(bottom, innerTop + parent.clientHeight);
        }
      }
      node = parent;
    }
    return right > left && bottom > top ? { left, top, width: right - left, height: bottom - top } : null;
  }

  function extentOf(element) {
    let { left, top, right, bottom } = element.getBoundingClientRect();
    const descendants = element.querySelectorAll("*");
    if (descendants.length <= WATCH_MAX_EXTENT_ELEMENTS) {
      for (const child of descendants) {
        const rect = child.getBoundingClientRect();
        if (
          rect.width < 1 ||
          rect.height < 1 ||
          rect.width > WATCH_BOX_MAX_WIDTH ||
          rect.height > WATCH_BOX_MAX_HEIGHT ||
          (rect.left >= left && rect.top >= top && rect.right <= right && rect.bottom <= bottom) ||
          !checkVisibleStrict(child)
        ) {
          continue;
        }
        const seen = watchVisibleRect(child, rect, { viewport: false });
        if (!seen) continue;
        left = Math.min(left, seen.left);
        top = Math.min(top, seen.top);
        right = Math.max(right, seen.left + seen.width);
        bottom = Math.max(bottom, seen.top + seen.height);
      }
    }
    return { left, top, right, bottom, width: right - left, height: bottom - top };
  }

  function boxAround(element, levels, growth) {
    const maxGrowth = growth === undefined ? Infinity : growth;
    const own = element.getBoundingClientRect();
    const most = own.width * own.height * maxGrowth;
    let box = element;
    let node = element.parentElement;
    for (let level = 0; node && level < levels; level++) {
      if (node === document.body || node === document.documentElement || node.localName === "form") break;
      const rect = node.getBoundingClientRect();
      const holds =
        rect.left <= own.left + 1 &&
        rect.top <= own.top + 1 &&
        rect.right >= own.right - 1 &&
        rect.bottom >= own.bottom - 1;
      if (!holds || rect.width > WATCH_BOX_MAX_WIDTH || rect.height > WATCH_BOX_MAX_HEIGHT || rect.width * rect.height > most) {
        break;
      }
      box = node;
      node = node.parentElement;
    }
    return box;
  }

  function widgetsOf(all) {
    try {
      const found = [...document.querySelectorAll(WIDGETS)];
      return all ? found : found.filter(element => !(element.parentElement && element.parentElement.closest(WIDGETS)));
    } catch (_) {
      return [];
    }
  }

  function promptTextNodes() {
    const lower = "translate(., 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz')";
    const words = PROMPT_WORDS.map(word => `contains(${lower}, '${word}')`).join(" or ");
    const query =
      `//body//text()[string-length(normalize-space(.)) > 3 and ` +
      `string-length(.) < ${PROMPT_MAX * 3} and (${words}) and ` +
      `not(ancestor::script or ancestor::style or ancestor::noscript or ` +
      `ancestor::template or ancestor::textarea or ancestor::title)]`;
    try {
      if (typeof document.evaluate === "function") {
        const found = document.evaluate(query, document, null, 7, null);
        const texts = [];
        const limit = Math.min(found.snapshotLength, WATCH_MAX_PROMPT_TEXTS);
        for (let i = 0; i < limit; i++) texts.push(found.snapshotItem(i));
        return texts;
      }
    } catch (_) {}
    const fallback = [];
    if (!document.body) return fallback;
    for (const node of deepTextNodes(document.body)) {
      if (fallback.length >= WATCH_MAX_PROMPT_TEXTS) break;
      const raw = node.data || "";
      if (raw.length >= PROMPT_MAX * 3 || raw.trim().length <= 3) continue;
      const lc = raw.toLowerCase();
      if (PROMPT_WORDS.some(w => lc.includes(w))) fallback.push(node);
    }
    return fallback;
  }

  function cleanText(text) {
    return String(text || "").replace(/\s+/g, " ").trim();
  }

  function ownChallenges() {
    if (!document.body || isCaptchaFrame()) return [];
    const found = [];
    for (const text of promptTextNodes()) {
      let element = text.parentElement;
      if (!element || element.closest(WATCH_NOT_PROMPT) || !checkVisibleStrict(element)) continue;
      const kind = promptKind(text.data) || promptKind(element.textContent);
      if (!kind) continue;
      const matched = element;
      for (
        let level = 0;
        level < WATCH_LINE_LEVELS &&
        element.parentElement &&
        element.parentElement !== document.body &&
        cleanText(element.parentElement.textContent).length <= WATCH_LINE_MAX;
        level++
      ) {
        element = element.parentElement;
      }
      if (element.closest(WIDGETS)) continue;
      const box = boxAround(element, WATCH_PROMPT_LEVELS);
      if (!watchIsShown(box) || !box.querySelector(WATCH_CONTROLS)) continue;
      const line = cleanText(element.innerText);
      const own = cleanText(matched.innerText || text.data);
      found.push({ box, kind, prompt: line.includes(own) ? line : own });
    }
    for (const element of document.querySelectorAll(
      "input[placeholder], input[aria-label], textarea[placeholder], img[alt], canvas[aria-label]"
    )) {
      const label = element.getAttribute("placeholder") || element.getAttribute("aria-label") || element.getAttribute("alt");
      const kind = promptKind(label);
      if (!kind || kind === "checkbox" || element.closest(WIDGETS) || !checkVisibleStrict(element)) continue;
      const box = boxAround(element, WATCH_PROMPT_LEVELS);
      if (watchIsShown(box) && (kind !== "text" || box.querySelector("img, canvas, svg"))) {
        found.push({ box, kind, prompt: cleanText(label) });
      }
    }
    for (const picture of document.querySelectorAll("img, canvas")) {
      const names = ["src", "alt", "id", "class", "name", "title", "aria-label"]
        .map(n => picture.getAttribute(n) || "")
        .join(" ");
      if (!namesOwnCaptcha(names) || picture.closest(WIDGETS) || !watchIsShown(picture)) continue;
      let box = picture.parentElement;
      for (let level = 0; box && level < OWN_CAPTCHA_LEVELS && !box.querySelector(WATCH_ANSWER_FIELDS); level++) {
        box = box.parentElement;
      }
      if (!box || !box.querySelector(WATCH_ANSWER_FIELDS)) continue;
      const r = box.getBoundingClientRect();
      if (r.width <= WATCH_BOX_MAX_WIDTH && r.height <= WATCH_BOX_MAX_HEIGHT) {
        found.push({ box, kind: "text", prompt: "picture and answer field" });
      }
    }
    const rank = item => (item.kind === "checkbox" ? 1 : 0);
    const kept = [];
    for (const item of found) {
      const other = kept.find(o => o.box === item.box || o.box.contains(item.box) || item.box.contains(o.box));
      if (!other) {
        kept.push(Object.assign({}, item));
        continue;
      }
      if (item.box.contains(other.box)) other.box = item.box;
      if (rank(item) < rank(other)) {
        other.kind = item.kind;
        other.prompt = item.prompt;
      }
    }
    return kept.filter(item => !kept.some(other => other !== item && other.box.contains(item.box))).slice(0, WATCH_MAX_OWN);
  }

  const inlineScriptUrlsCache = new WeakMap();
  const INLINE_URLS_RE = /https?:\/\/[^\s"'`<>()\\]+/g;

  function scriptUrls() {
    const urls = new Set();
    for (const script of document.scripts || []) {
      if (script.src) urls.add(script.src);
    }
    for (const link of document.querySelectorAll("link[rel=modulepreload][href], link[rel=preload][as=script][href]")) {
      if (link.href) urls.add(link.href);
    }
    let read = 0;
    for (const script of document.querySelectorAll("script:not([src])")) {
      if (/json|template|html/i.test(script.type || "")) continue;
      if (++read > WATCH_MAX_INLINE_SCRIPTS) break;
      let found = inlineScriptUrlsCache.get(script);
      if (!found) {
        found = [...String(script.text || "").slice(0, WATCH_MAX_INLINE_TEXT).matchAll(INLINE_URLS_RE)]
          .slice(0, WATCH_MAX_INLINE_URLS)
          .map(match => match[0]);
        inlineScriptUrlsCache.set(script, found);
      }
      for (const url of found) urls.add(url);
    }
    return [...urls];
  }

  function scanPage() {
    const detections = [];
    for (const frame of watchFramesOf()) {
      const src = frameSrcOf(frame);
      const name = src && nameOfUrl(src);
      if (name) {
        detections.push({
          signal: "frame",
          name,
          kind: kindOfUrl(src),
          challenge: isChallengeUrl(src),
          hidden: !watchIsShown(frame),
          detail: src.slice(0, 200),
        });
      }
    }
    for (const url of scriptUrls()) {
      const name = nameOfScriptUrl(url);
      if (name) {
        detections.push({
          signal: "script",
          name,
          kind: kindOfScriptUrl(url),
          challenge: false,
          hidden: false,
          detail: url.slice(0, 200),
        });
      }
    }
    const widgets = widgetsOf(false).filter(element => watchIsShown(element)).length;
    if (widgets) {
      detections.push({ signal: "widget", detail: String(widgets) });
    }
    for (const { kind, prompt } of ownChallenges()) {
      detections.push({ signal: "own", kind, detail: prompt.slice(0, 200) });
    }
    for (const field of document.querySelectorAll("input[name], textarea[name]")) {
      const name = String(field.name || "").slice(0, 64);
      if (isTokenField(name)) {
        detections.push({ signal: "token", detail: name });
        continue;
      }
      const visible = field.type !== "hidden" && isRendered(field);
      if (isHoneypotField(name, visible)) {
        detections.push({ signal: "honeypot", kind: "honeypot", detail: name });
      }
    }
    return detections;
  }

  const backgroundProbes = new WeakMap();
  const BACKGROUND_URLS = /url\(\s*(["']?)([^"')]+)\1\s*\)/g;
  const LOADER_NAMES = /(?:^|[-_\s])(?:loading|loader|spinner|spin|skeleton)(?:$|[-_\s])/i;
  const OVERLAY_NAMES = /(?:^|[-_\s])ai[-_]?detect(?:$|[-_\s])/i;
  const CHECKBOX_NAMES = /(?:^|[-_])(?:check(?:box)?|cb|tick)(?:$|[-_])/i;
  const CHECKED_NAMES = /(?:^|[-_])(?:checked|is-checked|done|success|verified|passed|solved)(?:$|[-_])/i;
  const WATCH_CLICKABLE = "button, [role=button], [role=checkbox], label, [tabindex], [onclick]";
  const MANAGED_PASSED = /^(?:success!?|verified|you are verified)$/i;
  const MANAGED_FAILED = /^(?:verification (?:failed|expired)|failed|expired|error)\b/i;
  const SELF_READ_KINDS = new Set(["checkbox", "managed", "proof-of-work", "score", "invisible"]);
  const STATUS_TEXT_MAX = 80;
  const EXPIRED_WORDS = /^(?:\S+\s+){0,3}?(?:expired|timed\s+out|time-?out)\b/i;
  const FAILED_WORDS = /^(?:\S+\s+){0,3}?(?:failed|failure|error)\b/i;
  const EXPIRED_NAMES = /(?:^|[-_])(?:expired?|expiry|timeout|timed-?out)(?:$|[-_])/i;
  const FAILED_NAMES = /(?:^|[-_])(?:fail|failed|failure|error)(?:$|[-_])/i;
  const PASSED_NAMES = /(?:^|[-_])(?:success|verified|passed|solved)(?:$|[-_])/i;
  const EXPIRED_STATES = new Set(["expired", "timeout"]);
  const FAILED_STATES = new Set(["error", "failed", "failure"]);
  const STATUS_RANK = { expired: 0, failed: 1, passed: 2 };
  const LOADING_NAMES = /(?:^|[-_])(?:loading|checking|busy)$/i;
  const LOADING_MAX_MS = 10000;
  let loadingSinceAt = null;
  let lastChallengeTarget = null;

  function elementNamesOf(element) {
    return `${element.id || ""} ${(element.getAttribute && element.getAttribute("class")) || ""}`;
  }

  function isLoader(element) {
    return LOADER_NAMES.test(elementNamesOf(element)) && checkVisibleStrict(element);
  }

  function backgroundsLoaded(root) {
    let probes = backgroundProbes.get(document);
    if (!probes) {
      probes = new Map();
      backgroundProbes.set(document, probes);
    }
    const elements = [root, ...root.querySelectorAll("*")];
    if (elements.length > WATCH_MAX_EXTENT_ELEMENTS) return true;
    for (const element of elements) {
      const painted = getComputedStyle(element) ? getComputedStyle(element).backgroundImage : "";
      if (!painted || !painted.includes("url(")) continue;
      const { width, height } = element.getBoundingClientRect();
      if (width < WATCH_MIN_SIDE || height < WATCH_MIN_SIDE) continue;
      if (!checkVisibleStrict(element)) continue;
      for (const match of painted.matchAll(BACKGROUND_URLS)) {
        const url = match[2];
        let probe = probes.get(url);
        if (!probe) {
          const image = new Image();
          probe = { ready: false };
          image.src = url;
          if (typeof image.decode === "function") {
            image.decode().then(
              () => {
                probe.ready = true;
              },
              () => {
                probe.ready = true;
              }
            );
          } else {
            image.onload = () => {
              probe.ready = true;
            };
            image.onerror = () => {
              probe.ready = true;
            };
          }
          probes.set(url, probe);
        }
        if (!probe.ready) return false;
      }
    }
    return true;
  }

  function picturesLoaded(element) {
    const images = [
      ...(element.localName === "img" ? [element] : []),
      ...element.querySelectorAll("img"),
    ].filter(image => checkVisibleStrict(image));
    return images.every(image => image.complete && image.naturalWidth > 0) && backgroundsLoaded(element);
  }

  function watchLoading(root) {
    const elements = [root, ...root.querySelectorAll("*")];
    if (elements.length > WATCH_MAX_EXTENT_ELEMENTS) return false;
    for (const element of elements) {
      if (element === root) continue;
      const { width, height } = element.getBoundingClientRect();
      if (width < WATCH_MIN_SIDE || height < WATCH_MIN_SIDE) continue;
      if (isLoader(element)) return true;
    }
    return false;
  }

  function watchArriving(element) {
    const ends = animation =>
      animation.playState === "running" &&
      animation.effect &&
      Number.isFinite(animation.effect.getComputedTiming().endTime);
    try {
      if (typeof element.getAnimations === "function" && element.getAnimations({ subtree: true }).some(ends)) {
        return true;
      }
      let opacity = 1;
      for (let node = element; node; node = node.parentElement) {
        if (node !== element && typeof node.getAnimations === "function" && node.getAnimations().some(ends)) {
          return true;
        }
        const st = getComputedStyle(node);
        opacity *= Number(st && st.opacity != null ? st.opacity : 1);
      }
      if (opacity < WATCH_FADED) return true;
    } catch (_) {}
    return false;
  }

  function picturesShown(element) {
    let count = 0;
    for (const picture of [element, ...element.querySelectorAll("*")]) {
      const tag = picture.localName;
      const mediaTag = tag === "img" || tag === "canvas" || tag === "video" || tag === "image";
      const painted = !mediaTag && picture.childElementCount <= 1;
      if (!mediaTag && !painted) continue;
      const { width, height } = picture.getBoundingClientRect();
      const bg = getComputedStyle(picture) ? getComputedStyle(picture).backgroundImage || "" : "";
      if (
        width < WATCH_PICTURE_SIDE ||
        height < WATCH_PICTURE_SIDE ||
        !checkVisibleStrict(picture) ||
        (painted && !bg.startsWith("url("))
      ) {
        continue;
      }
      if (tag !== "img" || (picture.complete && picture.naturalWidth > 0)) count++;
    }
    return count;
  }

  function watchClickable(element) {
    if (!checkVisibleStrict(element)) return false;
    const { width, height } = element.getBoundingClientRect();
    return width >= 8 && height >= 8;
  }

  function checkboxOf(root) {
    const real = [...root.querySelectorAll("[role=checkbox], input[type=checkbox]")].find(watchClickable);
    if (real) return real;
    for (const element of root.querySelectorAll("[aria-label], [title]")) {
      const label = element.getAttribute("aria-label") || element.getAttribute("title");
      if (watchClickable(element) && promptKind(label) === "checkbox") return element;
    }
    const named = [root, ...root.querySelectorAll("[class], [id]")].filter(
      element =>
        (CHECKBOX_NAMES.test(element.id || "") || [...(element.classList || [])].some(name => CHECKBOX_NAMES.test(name))) &&
        watchClickable(element)
    );
    const deepest = named.find(element => !named.some(other => other !== element && element.contains(other)));
    if (deepest) return deepest;
    for (const element of root.querySelectorAll("*")) {
      if (element.childElementCount <= 2 && watchClickable(element) && promptKind(element.innerText) === "checkbox") {
        const targetEl = element.closest(WATCH_CLICKABLE);
        return targetEl && root.contains(targetEl) ? targetEl : element;
      }
    }
    return null;
  }

  function isCheckedBox(control) {
    if (control.getAttribute("aria-checked") === "true" || control.checked === true) return true;
    return [control, control.parentElement].some(
      element => element && [...(element.classList || [])].some(name => CHECKED_NAMES.test(name))
    );
  }

  function managedCheckbox(peek, name) {
    let control = null;
    for (const element of deepElements(document)) {
      if (element.localName === "input" && element.type === "checkbox" && !control) {
        const holder = element.closest("label") || element;
        if (watchClickable(holder)) control = holder;
      } else if (element.childElementCount === 0 && checkVisibleStrict(element)) {
        const words = cleanText(element.textContent);
        if (MANAGED_PASSED.test(words)) return { passed: true };
        if (MANAGED_FAILED.test(words)) return { done: true };
      }
    }
    if (!control) return null;
    if (peek) return { checkbox: true };
    control.click();
    return { opened: true, name };
  }

  function frameCheckbox(peek) {
    if (window.parent === window) return null;
    const url = document.documentURI || location.href || "";
    const name = nameOfUrl(url);
    const kind = kindOfUrl(url);
    if (!name || isChallengeUrl(url) || (kind !== "checkbox" && kind !== "managed")) return null;
    if (kind === "managed") return managedCheckbox(peek, name);
    const control =
      document.querySelector("#recaptcha-anchor") ||
      document.querySelector("#checkbox") ||
      document.querySelector("[role='checkbox']") ||
      document.querySelector("input[type='checkbox']");
    if (!control) return null;
    if (control.getAttribute("aria-checked") === "true") return { passed: true };
    if (peek) return { checkbox: true };
    control.click();
    return { opened: true, name };
  }

  function openWidget() {
    const view = innerWidth * innerHeight;
    return (
      widgetsOf(true).find(widget => {
        const { width, height } = widget.getBoundingClientRect();
        return (
          width >= WATCH_OPEN_SIDE &&
          height >= WATCH_OPEN_SIDE &&
          width * height < view * WATCH_MOSTLY_VIEW &&
          watchIsShown(widget)
        );
      }) || null
    );
  }

  function holdsFrame(element, frames) {
    return frames.some(frame => {
      for (let node = frame; node; node = node.parentNode || node.host) {
        if (node === element) return true;
      }
      return false;
    });
  }

  function pageCheckbox(peek) {
    if (isCaptchaFrame()) return null;
    const own = ownChallenges();
    if (own.some(({ kind }) => kind !== "checkbox")) return null;
    if (openWidget()) return null;
    const roots = [
      ...own.map(({ box }) => ({ root: box, name: "page" })),
      ...widgetsOf(false)
        .filter(widget => watchIsShown(widget) && !widget.matches(NOT_OPENED) && !widget.querySelector("iframe, frame"))
        .map(widget => ({ root: widget, name: "widget" })),
    ];
    for (const { root, name } of roots) {
      const control = root.matches("button, [role=button], input[type=button]") ? root : checkboxOf(root);
      if (!control) continue;
      if (isCheckedBox(control)) return { passed: true };
      if (peek) return { checkbox: true };
      control.scrollIntoView({ block: "center", inline: "nearest", behavior: "instant" });
      control.click();
      return { opened: true, name };
    }
    return null;
  }

  function ownText(element) {
    let text = "";
    for (const node of element.childNodes || []) {
      if (node.nodeType === 3) text += " " + node.data;
    }
    return cleanText(text);
  }

  function* selfAndDeep(root) {
    if (root.nodeType === 1) {
      yield root;
      const shadow = shadowRootOf(root);
      if (shadow) yield* deepElements(shadow);
    }
    yield* deepElements(root);
  }

  function statusOfElement(element, passes) {
    const state = element.dataset && element.dataset.state ? element.dataset.state.toLowerCase() : "";
    if (state && EXPIRED_STATES.has(state)) return "expired";
    if (state && FAILED_STATES.has(state)) return "failed";
    const words = ownText(element);
    if (words && words.length <= STATUS_TEXT_MAX) {
      if (EXPIRED_WORDS.test(words)) return "expired";
      if (FAILED_WORDS.test(words)) return "failed";
      if (passes && MANAGED_PASSED.test(words)) return "passed";
    }
    const names = [element.id, ...(element.classList || [])].filter(Boolean);
    if (!names.length) return null;
    const rules = [
      ["expired", EXPIRED_NAMES],
      ["failed", FAILED_NAMES],
      ...(passes ? [["passed", PASSED_NAMES]] : []),
    ];
    const hit = rules.find(([, rule]) => names.some(name => rule.test(name)));
    const named = hit ? hit[0] : null;
    return named && cleanText(element.textContent) ? named : null;
  }

  function statusOf(root, passes) {
    let found = null;
    for (const element of selfAndDeep(root)) {
      const status = statusOfElement(element, !!passes);
      if (status && (!found || STATUS_RANK[status] < STATUS_RANK[found]) && checkVisibleStrict(element)) {
        found = status;
        if (found === "expired") break;
      }
    }
    return found;
  }

  function loadingFor() {
    const control = document.querySelector("#recaptcha-anchor, #checkbox, [role=checkbox]");
    const spinning =
      !!control &&
      (control.getAttribute("aria-busy") === "true" ||
        [...(control.classList || [])].some(name => LOADING_NAMES.test(name)));
    if (!spinning) {
      loadingSinceAt = null;
      return null;
    }
    if (loadingSinceAt === null) loadingSinceAt = Date.now();
    return Date.now() - loadingSinceAt;
  }

  function frameState() {
    const url = document.documentURI || location.href || "";
    const name = nameOfUrl(url);
    if (isChallengeUrl(url)) return { state: "none" };
    const kind = kindOfUrl(url);
    const answer = frameCheckbox(true);
    if (kind === "checkbox" && answer && answer.passed) {
      return { state: "solved", name, how: `${name}'s checkbox is ticked` };
    }
    const status = kind === "checkbox" || kind === "managed" ? statusOf(document, kind === "managed") : null;
    if (status === "expired") return { state: "failed", name, how: `${name} says the check expired` };
    if (status === "failed") return { state: "failed", name, how: `${name} says the check failed` };
    if ((answer && answer.passed) || status === "passed") {
      return { state: "solved", name, how: `${name} says the check passed` };
    }
    const spun = answer && answer.checkbox ? loadingFor() : null;
    if (spun != null && spun < LOADING_MAX_MS) {
      return { state: "checking", name, how: `${name} is checking the click` };
    }
    if (spun != null) {
      return {
        state: "unchecked",
        name,
        how: `${name}'s checkbox is stuck checking the click: click it again or reload`,
      };
    }
    if (answer && answer.done) {
      return { state: "failed", name, how: `${name} says the check failed or expired` };
    }
    if (answer && answer.checkbox) {
      return { state: "unchecked", name, how: `${name}'s checkbox is not ticked` };
    }
    if (kind === "managed" && innerWidth >= WATCH_MIN_SIDE && innerHeight >= WATCH_MIN_SIDE) {
      return { state: "checking", name, how: `${name} is checking on its own` };
    }
    return { state: "none" };
  }

  function pageState() {
    let best = { state: "none" };
    const take = reading => {
      if (reading.state !== best.state && strongerState(best.state, reading.state) === reading.state) {
        best = reading;
      }
    };
    const frames = watchFramesOf();
    for (const frame of frames) {
      const src = frameSrcOf(frame);
      const name = src && nameOfUrl(src);
      if (!name || !watchIsShown(frame)) continue;
      if (isChallengeUrl(src)) {
        take({ state: "open", name, how: `${name}'s challenge is open` });
      } else if (!SELF_READ_KINDS.has(kindOfUrl(src))) {
        take({ state: "shown", name, how: `${name} is shown` });
      }
    }
    const own = ownChallenges();
    for (const { kind, prompt } of own) {
      if (kind !== "checkbox") {
        take({
          state: "open",
          name: "page",
          how: `the page's challenge "${prompt.slice(0, 80)}" is shown`,
        });
      }
    }
    const popup = openWidget();
    if (popup && !holdsFrame(popup, frames)) {
      take({ state: "open", name: "widget", how: "a CAPTCHA's popup is open" });
    }
    const checkbox = pageCheckbox(true);
    if (checkbox && checkbox.checkbox) {
      take({ state: "unchecked", name: "page", how: "its checkbox is not ticked" });
    } else if (checkbox && checkbox.passed) {
      take({ state: "solved", name: "page", how: "its checkbox is ticked" });
    }
    for (const field of document.querySelectorAll("input[name], textarea[name]")) {
      if (!isTokenField(field.name)) continue;
      const fieldName = String(field.name || "").slice(0, 64);
      const filled = tokenStateOf(field.value);
      if (filled === "solved") {
        take({ state: "solved", how: `its answer field ${fieldName} is filled` });
      } else if (filled === "failed") {
        take({ state: "failed", how: `its answer field ${fieldName} says the check expired or failed` });
      } else if (filled === "checking") {
        take({ state: "checking", how: `its answer field ${fieldName} says the check is running` });
      }
    }
    for (const widget of widgetsOf(false)) {
      if (!watchIsShown(widget) || holdsFrame(widget, frames)) continue;
      const status = statusOf(widget, false);
      if (status === "expired" || status === "failed") {
        take({ state: "failed", name: "widget", how: `its widget says the check ${status}` });
      }
    }
    const marked = [];
    try {
      for (const widget of document.querySelectorAll(SOLVED_MARKS)) {
        const state = widget.dataset ? widget.dataset.state || "" : "";
        if (marksSolved(widget.classList, state) && isRendered(widget)) {
          marked.push(widget);
          take({ state: "solved", how: "its widget shows the success mark" });
        }
      }
    } catch (_) {}
    if (best.state === "none") {
      const shown = widgetsOf(false).some(
        widget =>
          watchIsShown(widget) &&
          !holdsFrame(widget, frames) &&
          !marked.some(mark => widget.contains(mark) || mark.contains(widget))
      );
      if (shown || own.length) {
        take({ state: "shown", name: "page", how: "a CAPTCHA is shown" });
      }
    }
    return best;
  }

  function captchaState() {
    try {
      if (!document.documentElement) return { state: "none" };
      return isCaptchaFrame() ? frameState() : pageState();
    } catch (_) {
      return { state: "none" };
    }
  }

  function openChallenge(peek) {
    return frameCheckbox(!!peek) || pageCheckbox(!!peek);
  }

  function challengeRect(stay) {
    if (!document.documentElement) return null;
    const frames = watchFramesOf().filter(frame => nameOfUrl(frameSrcOf(frame)));
    const own = ownChallenges();
    const kinds = [
      frames.filter(frame => isChallengeUrl(frameSrcOf(frame))),
      frames.filter(frame => !isChallengeUrl(frameSrcOf(frame))),
      widgetsOf(true),
      own.filter(({ kind }) => kind !== "checkbox").map(({ box }) => box),
      own.filter(({ kind }) => kind === "checkbox").map(({ box }) => box),
    ];
    const view = innerWidth * innerHeight;
    for (let kind = 0; kind < kinds.length; kind++) {
      const shown = kinds[kind]
        .filter(element => watchIsShown(element))
        .map(element => {
          const { width, height } = element.getBoundingClientRect();
          return { element, area: width * height };
        })
        .sort((a, b) => b.area - a.area);
      const ordered = [
        ...shown.filter(({ area }) => area < view * WATCH_MOSTLY_VIEW),
        ...shown.filter(({ area }) => area >= view * WATCH_MOSTLY_VIEW),
      ];
      for (const { element } of ordered) {
        let targetEl = element;
        if (kind === 2) {
          const { width, height } = element.getBoundingClientRect();
          targetEl =
            width < WATCH_SMALL_WIDTH || height < WATCH_SMALL_HEIGHT
              ? boxAround(element, WATCH_WIDGET_LEVELS)
              : boxAround(element, WATCH_WIDGET_LEVELS, WATCH_WIDGET_GROWTH);
        }
        const isFrame = kind <= 1;
        lastChallengeTarget = targetEl;
        const reach = () => (isFrame ? targetEl.getBoundingClientRect() : extentOf(targetEl));
        let full = reach();
        if (!stay) {
          targetEl.scrollIntoView({
            block: full.height > innerHeight ? "start" : "center",
            inline: "nearest",
            behavior: "instant",
          });
          full = reach();
          if (full.top < 0 || full.bottom > innerHeight) {
            scrollBy({ top: full.top - Math.max(0, (innerHeight - full.height) / 2), behavior: "instant" });
            full = reach();
          }
        }
        const rect = watchVisibleRect(targetEl, full);
        if (!rect || rect.width < WATCH_MIN_SIDE || rect.height < WATCH_MIN_SIDE) continue;
        const fInfo = isFrame ? frameInfoOf(element) : null;
        return Object.assign({}, rect, {
          fullWidth: full.width,
          fullHeight: full.height,
          viewWidth: innerWidth,
          viewHeight: innerHeight,
          scrollX,
          scrollY,
          challenge: kind === 0 || kind === 3,
          frame: isFrame,
          own: kind >= 3,
          inside: isCaptchaFrame(),
          drawn: isFrame || (picturesLoaded(targetEl) && !watchArriving(targetEl) && !watchLoading(targetEl)),
          arriving: watchArriving(targetEl),
          pictures: isFrame ? null : picturesShown(targetEl),
          frameId: fInfo ? fInfo.id : null,
        });
      }
    }
    return null;
  }

  function challengeDrawn(any) {
    const url = document.documentURI || location.href || "";
    if (!(any ? nameOfUrl(url) : isChallengeUrl(url)) || window.parent === window) return null;
    const body = document.body;
    if (document.readyState !== "complete" || !body || watchArriving(document.documentElement) || watchLoading(body)) {
      return { drawn: false };
    }
    const images = [...(document.images || [])].filter(image => checkVisibleStrict(image));
    if (!backgroundsLoaded(body)) return { drawn: false };
    if (images.length) {
      return { drawn: images.every(image => image.complete && image.naturalWidth > 0) };
    }
    const content =
      body.querySelector("canvas, button, [role='button'], input, table") ||
      (body.textContent || "").trim() ||
      [body, ...body.querySelectorAll("*")].some(element => shadowRootOf(element));
    return { drawn: !!content };
  }

  // Live CAPTCHA watch lifecycle (`ZendiaCaptchaWatchChild`, lines 2176-2397)
  const WATCH_RESCAN_DELAY_MS = 500;
  const WATCH_RESHOW_DELAY_MS = 1000;
  const WATCH_LATE_LOOKS_MS = [1500, 4000];
  const WATCH_STATE_POLL_MS = 1500;
  const WATCH_ADDED_TAGS = new Set(["iframe", "frame", "script", "input", "textarea"]);
  const WATCH_SHOWN_BY = ["style", "class", "hidden", "open", "aria-hidden", "aria-checked", "data-state"];
  const WATCH_NOTHING = JSON.stringify([[], { state: "none" }]);

  let watchObserver = null;
  let watchTimer = 0;
  let watchReshowTimer = 0;
  let watchLateTimers = [];
  let watchStateTimer = 0;
  let watchSent = WATCH_NOTHING;
  let watchLastState = "";

  function watchBringsCaptcha(records) {
    for (const record of records) {
      for (const node of record.addedNodes || []) {
        if (
          node.nodeType === 1 &&
          (WATCH_ADDED_TAGS.has(node.localName) ||
            (node.querySelector && node.querySelector("iframe, frame, script, input, textarea, img, canvas")))
        ) {
          return true;
        }
      }
    }
    return false;
  }

  function watchShowsSomething(records) {
    return records.some(
      record => record.type === "attributes" || (record.type === "childList" && record.addedNodes && record.addedNodes.length)
    );
  }

  function watchFollowState(found) {
    if (!found) {
      if (watchStateTimer) {
        clearInterval(watchStateTimer);
        watchStateTimer = 0;
      }
      return;
    }
    if (watchStateTimer) return;
    watchStateTimer = setInterval(() => {
      if (JSON.stringify(captchaState()) !== watchLastState) {
        watchScan(false);
      }
    }, WATCH_STATE_POLL_MS);
  }

  function watchScan(force) {
    if (!document) return;
    const detections = scanPage();
    const state = captchaState();
    const found = detections.length > 0 || state.state !== "none";
    const sent = JSON.stringify([detections, state]);
    if (force ? found : sent !== watchSent) {
      const info = docInfo();
      postEvent({
        kind: "captcha_watch_report",
        frame_id: info.frameId,
        top: !!info.top,
        url: info.url || String(document.documentURI || location.href || ""),
        detections,
        state,
      });
    }
    watchSent = sent;
    watchLastState = JSON.stringify(state);
    watchFollowState(found || isCaptchaFrame());
  }

  function watchStop() {
    if (watchObserver) {
      watchObserver.disconnect();
      watchObserver = null;
    }
    if (watchTimer) {
      clearTimeout(watchTimer);
      watchTimer = 0;
    }
    if (watchReshowTimer) {
      clearTimeout(watchReshowTimer);
      watchReshowTimer = 0;
    }
    if (watchStateTimer) {
      clearInterval(watchStateTimer);
      watchStateTimer = 0;
    }
    for (const t of watchLateTimers) clearTimeout(t);
    watchLateTimers = [];
  }

  function watchStart(forceScan) {
    if (forceScan) watchScan(true);
    if (watchObserver || typeof MutationObserver !== "function" || !document.documentElement) return;
    watchObserver = new MutationObserver(records => {
      if (watchBringsCaptcha(records)) {
        initChildFrames(false);
        clearTimeout(watchTimer);
        watchTimer = setTimeout(() => watchScan(false), WATCH_RESCAN_DELAY_MS);
      } else if (watchShowsSomething(records) && !watchReshowTimer) {
        watchReshowTimer = setTimeout(() => {
          watchReshowTimer = 0;
          watchScan(false);
        }, WATCH_RESHOW_DELAY_MS);
      }
    });
    watchObserver.observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: WATCH_SHOWN_BY,
    });
    watchLateTimers = WATCH_LATE_LOOKS_MS.map(delay =>
      setTimeout(() => {
        initChildFrames(false);
        watchScan(false);
      }, delay)
    );
  }

  window.addEventListener("pageshow", () => {
    watchStart(true);
    initChildFrames(false);
  });
  window.addEventListener("pagehide", () => {
    watchStop();
    watchSent = WATCH_NOTHING;
    watchLastState = "";
    const info = docInfo();
    postEvent({ kind: "captcha_watch_gone", frame_id: info.frameId });
  });

  function drawnChallenges() {
    const now = Date.now();
    if (drawnChallengesCache.at && now - drawnChallengesCache.at < DRAWN_CACHE_MS) {
      return drawnChallengesCache.boxes.filter(box => box && box.isConnected);
    }
    let boxes = [];
    try {
      boxes = ownChallenges().map(item => item.box);
    } catch (_) {}
    drawnChallengesCache = { at: now, boxes };
    return boxes;
  }

  function widenOwnBox(box) {
    const base = box.getBoundingClientRect();
    let best = box;
    for (
      let each = box.parentElement, level = 0;
      each && each !== document.body && each !== document.documentElement && level < OWN_CAPTCHA_WIDEN;
      each = each.parentElement, level++
    ) {
      const rect = each.getBoundingClientRect();
      if (
        rect.height > base.height * 2 + 120 ||
        rect.width * rect.height > Math.max(base.width * base.height * 12, 160000) ||
        [...each.querySelectorAll(OTHER_FIELDS)].some(field => !box.contains(field))
      ) {
        break;
      }
      best = each;
      if (each.localName === "form") break;
    }
    return best;
  }

  function ownCaptchas() {
    const named = [];
    try {
      for (const element of document.querySelectorAll("img, canvas, input:not([type=hidden]), textarea")) {
        const attrs = ["src", "alt", "id", "class", "name", "placeholder", "aria-label", "title"]
          .map(n => element.getAttribute(n) || "")
          .join(" ");
        if (!namesOwnCaptcha(attrs)) continue;
        const rect = element.getBoundingClientRect();
        const field = element.localName === "input" || element.localName === "textarea";
        const least = field ? { width: 20, height: 10 } : OWN_PICTURE_MIN;
        if (
          rect.width >= least.width &&
          rect.height >= least.height &&
          rect.bottom > 0 &&
          rect.top < innerHeight &&
          isRendered(element) &&
          !element.closest(WIDGETS) &&
          (field || !element.closest("a[href]"))
        ) {
          named.push(element);
        }
      }
    } catch (_) {}
    const pics = named.filter(e => e.localName !== "input" && e.localName !== "textarea");
    const flds = named.filter(e => !pics.includes(e));
    const boxes = new Set();
    for (const field of flds) {
      let box = field.parentElement;
      for (let level = 0; box && level < OWN_CAPTCHA_LEVELS && !pics.some(p => box.contains(p)); level++) {
        box = box.parentElement;
      }
      boxes.add(box && pics.some(p => box.contains(p)) ? box : field.parentElement || field);
    }
    for (const pic of pics) {
      if (![...boxes].some(b => b.contains(pic))) boxes.add(pic.parentElement || pic);
    }
    const widened = new Set([...boxes].filter(Boolean).map(widenOwnBox));
    for (const box of drawnChallenges()) widened.add(box);
    const list = [...widened];
    return list.filter(box => !list.some(other => other !== box && other.contains(box)));
  }

  function inPageCaptcha(element, own) {
    for (let each = element; each; each = composedParent(each)) {
      if (each.matches && each.matches(WIDGETS)) return true;
    }
    return own.some(box => composedContains(box, element));
  }

  function purposeOfElement(element) {
    return purposeOfControl({
      tag: element.localName,
      type: element.getAttribute("type") || "",
      editable: isEditable(element),
      label: textOf(element) + " " + (element.getAttribute("title") || ""),
      name: (element.id || "") + " " + (element.getAttribute("class") || ""),
    });
  }

  function isPictureRect(rect) {
    return rect.width >= 150 && rect.height >= 100;
  }

  function visibleElementRect(element) {
    const rect = element.getBoundingClientRect();
    let left = rect.left;
    let top = rect.top;
    let right = rect.right;
    let bottom = rect.bottom;
    for (let each = composedParent(element), level = 0; each && level < 12; each = composedParent(each), level++) {
      const st = getComputedStyle(each);
      if (!st || (st.overflowX === "visible" && st.overflowY === "visible")) continue;
      const box = each.getBoundingClientRect();
      if (st.overflowX !== "visible") {
        left = Math.max(left, box.left);
        right = Math.min(right, box.right);
      }
      if (st.overflowY !== "visible") {
        top = Math.max(top, box.top);
        bottom = Math.min(bottom, box.bottom);
      }
    }
    if (right <= left || bottom <= top) return rect;
    return { left, top, width: right - left, height: bottom - top, right, bottom };
  }

  function isUncoveredCenter(element, rect) {
    const cx = Math.min(Math.max(rect.left + rect.width / 2, 0), Math.max(innerWidth - 1, 0));
    const cy = Math.min(Math.max(rect.top + rect.height / 2, 0), Math.max(innerHeight - 1, 0));
    const hit = deepElementFromPoint(cx, cy);
    return !hit || hit === element || composedContains(element, hit) || composedContains(hit, element);
  }

  function findBackgroundPictures(root) {
    const scope = root || document;
    const found = [];
    try {
      const elements = scope === document ? deepElements(document.body || document.documentElement) : deepElements(scope);
      let checked = 0;
      for (const element of elements) {
        if (++checked > DRAWN_MAX) break;
        if (element.localName === "img" || element.localName === "canvas" || element.childElementCount > 2) continue;
        const rect = element.getBoundingClientRect();
        if (
          rect.width < PICTURE_MIN.width ||
          rect.height < PICTURE_MIN.height ||
          rect.bottom <= 0 ||
          rect.right <= 0 ||
          rect.top >= innerHeight ||
          rect.left >= innerWidth ||
          !isRendered(element)
        ) {
          continue;
        }
        const bg = getComputedStyle(element).backgroundImage || "";
        if (bg.startsWith("url(") && isUncoveredCenter(element, rect)) {
          found.push(element);
        }
      }
    } catch (_) {}
    return found;
  }

  function findPictures(selector, root) {
    const scope = root || document;
    const found = [];
    try {
      for (const element of scope.querySelectorAll(selector)) {
        const rect = element.getBoundingClientRect();
        if (
          rect.width >= PICTURE_MIN.width &&
          rect.height >= PICTURE_MIN.height &&
          rect.bottom > 0 &&
          rect.right > 0 &&
          rect.top < innerHeight &&
          rect.left < innerWidth &&
          isRendered(element) &&
          isUncoveredCenter(element, rect)
        ) {
          found.push(element);
        }
      }
    } catch (_) {}
    return found;
  }

  function drawnRect(element) {
    const box = element.getBoundingClientRect();
    let left = box.left;
    let top = box.top;
    let right = box.right;
    let bottom = box.bottom;
    const clips = new Map([[composedParent(element), null]]);
    const clipOf = node => {
      if (!node) return null;
      if (clips.has(node)) return clips.get(node);
      const above = clipOf(composedParent(node));
      let clip = above;
      const st = getComputedStyle(node);
      const clipsX = st && st.overflowX !== "visible";
      const clipsY = st && st.overflowY !== "visible";
      if (clipsX || clipsY) {
        const own = node.getBoundingClientRect();
        clip = {
          left: clipsX ? own.left : -Infinity,
          top: clipsY ? own.top : -Infinity,
          right: clipsX ? own.right : Infinity,
          bottom: clipsY ? own.bottom : Infinity,
        };
        if (above) {
          clip = {
            left: Math.max(clip.left, above.left),
            top: Math.max(clip.top, above.top),
            right: Math.min(clip.right, above.right),
            bottom: Math.min(clip.bottom, above.bottom),
          };
        }
      }
      clips.set(node, clip);
      return clip;
    };
    let looked = 0;
    const roots = [element, shadowRootOf(element)].filter(Boolean);
    for (const root of roots) {
      for (const each of deepElements(root)) {
        if (++looked > DRAWN_MAX) break;
        const rect = each.getBoundingClientRect();
        if (rect.width < 4 || rect.height < 4 || !checkVisibleStrict(each)) continue;
        const clip = clipOf(composedParent(each)) || {
          left: -Infinity,
          top: -Infinity,
          right: Infinity,
          bottom: Infinity,
        };
        const shownLeft = Math.max(rect.left, clip.left);
        const shownTop = Math.max(rect.top, clip.top);
        const shownRight = Math.min(rect.right, clip.right);
        const shownBottom = Math.min(rect.bottom, clip.bottom);
        if (shownRight - shownLeft < 4 || shownBottom - shownTop < 4) continue;
        left = Math.min(left, shownLeft);
        top = Math.min(top, shownTop);
        right = Math.max(right, shownRight);
        bottom = Math.max(bottom, shownBottom);
      }
    }
    left = Math.max(left, 0);
    top = Math.max(top, 0);
    right = Math.min(right, innerWidth);
    bottom = Math.min(bottom, innerHeight);
    if (right <= left || bottom <= top) {
      return { left: box.left, top: box.top, width: box.width, height: box.height, right: box.right, bottom: box.bottom };
    }
    return { left, top, width: right - left, height: bottom - top, right, bottom };
  }

  function outermostRects(rects) {
    return rects.filter(
      rect =>
        !rects.some(
          other =>
            other !== rect &&
            other.left <= rect.left &&
            other.top <= rect.top &&
            other.right >= rect.right &&
            other.bottom >= rect.bottom &&
            (other.width > rect.width || other.height > rect.height)
        )
    );
  }

  async function captchaAreas() {
    const found = [];
    const frameResults = await Promise.all(
      visibleFrames().map(async frame => {
        const box = frameBox(frame);
        if (nameOfUrl(frameSrcOf(frame)) !== null) {
          return [box];
        }
        const sub = await evalInFrame(frame, { op: "captcha_areas" });
        if (!sub || !Array.isArray(sub.areas)) return [];
        const nested = [];
        for (const area of sub.areas) {
          const mapped = fromFrame(box, area);
          if (mapped.right > box.left && mapped.left < box.right && mapped.bottom > box.top && mapped.top < box.bottom) {
            nested.push(mapped);
          }
        }
        return nested;
      })
    );
    for (const list of frameResults) found.push(...list);
    try {
      const all = [...document.querySelectorAll(WIDGETS), ...ownCaptchas()];
      const outer = all.filter(el => !all.some(other => other !== el && other.contains(el)));
      for (const element of outer) {
        const rect = drawnRect(element);
        if (isRendered(element) && rect.width >= 20 && rect.height >= 20 && rect.bottom > 0 && rect.top < innerHeight) {
          found.push(rect);
        }
      }
    } catch (_) {}
    return outermostRects(found).map(r => ({ x: r.left, y: r.top, width: r.width, height: r.height }));
  }

  async function captchaTokens() {
    let count = 0;
    try {
      for (const field of document.querySelectorAll("input[name], textarea[name]")) {
        if (field.value && isTokenField(field.name)) count++;
      }
      for (const widget of document.querySelectorAll(SOLVED_MARKS)) {
        const state = widget.dataset ? widget.dataset.state || "" : "";
        if (marksSolved(widget.classList, state) && isRendered(widget)) count++;
      }
    } catch (_) {}
    const subCounts = await Promise.all(
      visibleFrames().map(async frame => {
        const sub = await evalInFrame(frame, { op: "captcha_tokens" });
        return sub && typeof sub.tokens === "number" ? sub.tokens : 0;
      })
    );
    for (const n of subCounts) count += n;
    return count;
  }

  // Raw frame control collection (`ZendiaJumpContent.collectControls`), including bare `<canvas>` pictures for parent frames.
  async function collectControlsRaw() {
    const own = ownCaptchas();
    const offered = new Map();
    const controls = findControls();
    const items = [];
    for (const element of controls) {
      const id = controlId(element);
      offered.set(id, { control: element });
      let box = element.getBoundingClientRect();
      if (isPictureRect(box)) box = visibleElementRect(element);
      const slider = isSlider(element);
      const tr = slider ? trackOf(element) : null;
      items.push({
        id,
        text: textOf(element),
        x: box.left,
        y: box.top,
        width: box.width,
        height: box.height,
        field: isField(element),
        href: element.localName === "a" && element.href ? element.href : null,
        captcha: inPageCaptcha(element, own),
        challenge: false,
        drag: isDraggable(element),
        slider,
        track: tr ? { x: tr.x, y: tr.y, width: tr.width, height: tr.height } : null,
        canvas: false,
        purpose: purposeOfElement(element),
      });
    }
    const known = new Set(items.map(i => i.id));
    const pictureCandidates = [
      ...findPictures("canvas", document),
      ...(isCaptchaFrame() ? findPictures("img", document) : []),
      ...(isCaptchaFrame() ? findBackgroundPictures(document) : []),
    ];
    for (const pic of pictureCandidates) {
      const id = controlId(pic);
      if (known.has(id) || controls.some(each => each.contains(pic))) continue;
      known.add(id);
      offered.set(id, { pictureOnly: pic });
      const box = visibleElementRect(pic);
      items.push({
        id,
        text: "",
        x: box.left,
        y: box.top,
        width: box.width,
        height: box.height,
        field: false,
        href: null,
        captcha: isCaptchaFrame() || inPageCaptcha(pic, own),
        challenge: false,
        drag: false,
        slider: false,
        track: null,
        canvas: true,
        purpose: null,
      });
    }
    const frameBatches = await Promise.all(
      visibleFrames().map(async frame => {
        const box = frameBox(frame);
        const fSrc = frameSrcOf(frame);
        const sub = await evalInFrame(frame, { op: "collect_controls" });
        return { frame, box, fSrc, sub };
      })
    );
    for (const { frame, box, fSrc, sub } of frameBatches) {
      if (!sub || !Array.isArray(sub.items)) continue;
      const prefix = frameId(frame) + "/";
      for (const item of sub.items) {
        const mapped = fromFrame(box, item);
        if (!uncoveredInFrame(frame, box, mapped)) continue;
        const id = prefix + item.id;
        offered.set(id, { frame, childId: item.id });
        const mappedTrack = item.track ? fromFrame(box, item.track) : null;
        items.push({
          id,
          text: String(item.text || "").slice(0, 200),
          x: mapped.left,
          y: mapped.top,
          width: mapped.width,
          height: mapped.height,
          field: !!item.field,
          href: item.href || null,
          challenge: isChallengeUrl(fSrc) || item.challenge === true,
          captcha: nameOfUrl(fSrc) !== null || item.captcha === true,
          drag: item.drag === true,
          slider: item.slider === true,
          track: mappedTrack ? { x: mappedTrack.x, y: mappedTrack.y, width: mappedTrack.width, height: mappedTrack.height } : null,
          canvas: item.canvas === true,
          purpose: PURPOSES.includes(item.purpose) ? item.purpose : null,
        });
      }
    }
    pickable = offered;
    return items;
  }

  // The controls of the page and its frames (`FRM-01`, `CAP-01`..`CAP-07`), with `scope` `"all" | "challenge" | "captcha"`
  // and `fields` `"include" | "exclude" | "only"`.
  async function controlTargets(fields, pattern, scope, topOnly) {
    const modeScope = scope || "all";
    const from = center();
    const own = ownCaptchas();
    const targets = [];
    const offered = new Map();
    const localControls = findControls();
    for (const element of localControls) {
      const inCap = inPageCaptcha(element, own);
      const field = isField(element);
      if (modeScope !== "all" && !inCap) continue;
      if (fields !== "include" && field !== (fields === "only")) continue;
      const text = pattern ? textOf(element) : "";
      const found = pattern ? findMatches(text, pattern) : [];
      if (pattern && !found.length) continue;
      let rect = element.getBoundingClientRect();
      if (inCap && isPictureRect(rect)) rect = visibleElementRect(element);
      const id = controlId(element);
      offered.set(id, { control: element });
      const slider = inCap && isSlider(element);
      const tr = slider ? trackOf(element) : null;
      targets.push(
        target(id, rect, field ? "field" : "control", {
          nexts: found.map(match => match.next),
          distance: distance(rect, from),
          href: element.localName === "a" && element.href ? element.href : null,
          challenge: inCap,
          drag: inCap && isDraggable(element),
          slider,
          track: tr ? { x: tr.x, y: tr.y, width: tr.width, height: tr.height } : null,
          picture: inCap && isPictureRect(rect),
          purpose: inCap ? purposeOfElement(element) : null,
        })
      );
    }
    if (modeScope !== "all" && fields !== "only" && !pattern) {
      const seen = new Set(localControls);
      const roots = [...own];
      try {
        roots.push(...document.querySelectorAll(WIDGETS));
      } catch (_) {}
      for (const root of roots) {
        for (const pic of [...findPictures("img, canvas", root), ...findBackgroundPictures(root)]) {
          if (seen.has(pic) || localControls.some(each => each.contains(pic))) continue;
          seen.add(pic);
          const rect = visibleElementRect(pic);
          const id = controlId(pic);
          offered.set(id, { pictureOnly: pic });
          targets.push(
            target(id, rect, "control", {
              distance: distance(rect, from),
              challenge: true,
              drag: false,
              slider: false,
              track: null,
              picture: true,
              purpose: null,
            })
          );
        }
      }
    }
    const frameBatches = topOnly
      ? []
      : await Promise.all(
          visibleFrames().map(async frame => {
            const box = frameBox(frame);
            const fSrc = frameSrcOf(frame);
            const sub = await evalInFrame(frame, { op: "collect_controls" });
            return { frame, box, fSrc, sub };
          })
        );
    for (const { frame, box, fSrc, sub } of frameBatches) {
      if (!sub || !Array.isArray(sub.items)) continue;
      const prefix = frameId(frame) + "/";
      for (const item of sub.items) {
        if (fields !== "include" && !!item.field !== (fields === "only")) continue;
        const itemChallenge = isChallengeUrl(fSrc) || item.challenge === true;
        const itemCaptcha = nameOfUrl(fSrc) !== null || item.captcha === true;
        if (modeScope === "all") {
          if (item.canvas) continue;
        } else if (modeScope === "challenge") {
          if (!itemChallenge) continue;
        } else if (!itemCaptcha) {
          continue;
        }
        const found = pattern ? findMatches(item.text || "", pattern) : [];
        if (pattern && !found.length) continue;
        const mapped = fromFrame(box, item);
        if (!uncoveredInFrame(frame, box, mapped)) continue;
        const id = prefix + item.id;
        offered.set(id, { frame, childId: item.id });
        const inCap = itemChallenge || itemCaptcha;
        const slider = !!item.slider && inCap;
        const mappedTrack = slider && item.track ? fromFrame(box, item.track) : null;
        targets.push(
          target(id, mapped, item.field ? "field" : "control", {
            nexts: found.map(match => match.next),
            distance: distance(mapped, from),
            href: item.href || null,
            challenge: modeScope === "captcha" || itemChallenge,
            drag: !!item.drag && inCap,
            slider,
            track: mappedTrack ? { x: mappedTrack.x, y: mappedTrack.y, width: mappedTrack.width, height: mappedTrack.height } : null,
            picture: inCap && (!!item.canvas || isPictureRect(mapped)),
            purpose: inCap && PURPOSES.includes(item.purpose) ? item.purpose : null,
          })
        );
      }
    }
    pickable = offered;
    return targets;
  }

  const MAX_SEGMENTS = 3000;
  const MAX_MATCHES = 500;

  function caretPoint() {
    const selection = getSelection();
    if (!selection || !selection.focusNode) return null;
    try {
      const range = document.createRange();
      range.setStart(selection.focusNode, selection.focusOffset);
      const rect = range.getBoundingClientRect();
      return rect.height ? { x: rect.left, y: rect.top } : null;
    } catch (_) {
      return null;
    }
  }

  // The places a pattern occurs in the text on screen (`S`), frames included (`FRM-01`), nearest the cursor first.
  async function textTargets(pattern) {
    const from = caretPoint() || center();
    const visible = new Map();
    const ancVisible = new Map();
    const clips = new Map();
    const segments = [];
    for (const node of deepTextNodes(document.body || document.documentElement)) {
      if (segments.length >= MAX_SEGMENTS) break;
      const parent = composedParent(node);
      if (!node.data.trim() || !parent || !isTextRendered(parent, visible, ancVisible)) continue;
      const box = parent.getBoundingClientRect();
      const contents = getComputedStyle(parent).display === "contents";
      if (contents || (box.bottom > 0 && box.top < innerHeight && box.right > 0 && box.left < innerWidth)) {
        segments.push(node);
      }
    }
    const targets = [];
    const offered = new Map();
    outer: for (let s = 0; s < segments.length; s++) {
      const node = segments[s];
      for (const match of findMatches(node.data, pattern)) {
        const range = document.createRange();
        range.setStart(node, match.index);
        range.setEnd(node, match.index + pattern.length);
        const rect = firstRangeRect(range);
        if (rect && onScreen(rect) && wordShows(range, rect, clips)) {
          const id = s + ":" + match.index;
          offered.set(id, { caret: { node, offset: match.index } });
          targets.push(target(id, rect, "match", { nexts: [match.next], distance: distance(rect, from) }));
        }
        if (targets.length >= MAX_MATCHES) break outer;
      }
    }
    if (targets.length < MAX_MATCHES) {
      const frameBatches = await Promise.all(
        visibleFrames().map(async frame => ({
          frame,
          box: frameBox(frame),
          sub: await evalInFrame(frame, { op: "jump_scan", jump: "text", pattern }),
        }))
      );
      for (const { frame, box, sub } of frameBatches) {
        if (targets.length >= MAX_MATCHES) break;
        if (!sub || !Array.isArray(sub.targets)) continue;
        const prefix = frameId(frame) + "/";
        for (const item of sub.targets) {
          const mapped = fromFrame(box, item);
          if (!onScreen(mapped) || !uncoveredInFrame(frame, box, mapped)) continue;
          const id = prefix + item.id;
          offered.set(id, { frame, childId: item.id, isVisual: true });
          targets.push(
            target(id, mapped, "match", {
              nexts: Array.isArray(item.nexts) ? item.nexts : [],
              distance: distance(mapped, from),
            })
          );
          if (targets.length >= MAX_MATCHES) break;
        }
      }
    }
    pickable = offered;
    return targets;
  }

  let WORD;
  try {
    WORD = new RegExp("[\\p{L}\\p{N}_]+", "gu");
  } catch (_) {
    WORD = /[A-Za-z0-9_À-￿]+/g;
  }
  // The most labels at once: two letters of the alphabet make 576.
  const MAX_WORD_TARGETS = 560;
  const PAINTED_ELEMENTS = new Set(["img", "video", "canvas", "iframe", "svg", "embed", "object", "picture"]);
  const INTERACTIVE_ANCESTORS =
    "a[href], button, label, summary, [role=button], [role=link], [role=tab], [role=menuitem], [role=option], [role=radio], [role=checkbox], [role=switch]";

  function firstRangeRect(range) {
    if (!range) return null;
    const clientRects = range.getClientRects();
    for (let i = 0; i < clientRects.length; i++) {
      if (clientRects[i].width > 0 && clientRects[i].height > 0) return clientRects[i];
    }
    const rect = range.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0 ? rect : null;
  }

  // Intersection of overflow-clipping boxes from `element` upwards (leaving inline / `display: contents` elements
  // unclipped by their own bounding rect so multi-link words and wrapped inline spans are preserved, `VIS-04`).
  function clippedRect(element, cache) {
    if (cache.has(element)) return cache.get(element);
    let left = -Infinity;
    let top = -Infinity;
    let right = Infinity;
    let bottom = Infinity;
    for (
      let each = element, level = 0;
      each && each !== document.body && each !== document.documentElement && level < 16;
      each = composedParent(each), level++
    ) {
      if (each.nodeType !== 1) continue;
      const style = getComputedStyle(each);
      if (!style || style.display === "inline" || style.display === "contents") continue;
      if (style.overflowX === "visible" && style.overflowY === "visible") continue;
      const box = each.getBoundingClientRect();
      if (style.overflowX !== "visible") {
        left = Math.max(left, box.left);
        right = Math.min(right, box.right);
      }
      if (style.overflowY !== "visible") {
        top = Math.max(top, box.top);
        bottom = Math.min(bottom, box.bottom);
      }
    }
    const clipped = { left, top, right, bottom };
    cache.set(element, clipped);
    return clipped;
  }

  function paintsOver(element) {
    if (PAINTED_ELEMENTS.has(element.localName)) return true;
    const style = getComputedStyle(element);
    if (!style) return false;
    if (style.backgroundImage && style.backgroundImage !== "none") return true;
    const color = style.backgroundColor || "";
    if (!color || color === "transparent") return false;
    const parts = /\(([^)]*)\)/.exec(color);
    const values = parts ? parts[1].split(/[\s,/]+/).filter(Boolean) : [];
    if (values.length < 4) return true;
    const alpha = values[3].endsWith("%") ? parseFloat(values[3]) / 100 : parseFloat(values[3]);
    return alpha >= 0.5;
  }

  function coveredBy(hit, startEl, endEl) {
    for (
      let each = hit;
      each && !composedContains(each, startEl) && (!endEl || !composedContains(each, endEl));
      each = composedParent(each)
    ) {
      if (each.nodeType === 1 && paintsOver(each)) return true;
    }
    return false;
  }

  // Whether a word range is genuinely visible on screen: unclipped by `overflow: hidden`/`scroll` ancestors and not
  // painted over by an opaque foreground card, modal, or header, while preserving buttons and adjacent links (`VIS-04`).
  function wordShows(range, rect, clips) {
    const startEl = range.startContainer.nodeType === 1 ? range.startContainer : composedParent(range.startContainer);
    if (!startEl || startEl.nodeType !== 1) return false;
    const endEl = range.endContainer.nodeType === 1 ? range.endContainer : composedParent(range.endContainer);
    const clip = clippedRect(startEl, clips);
    const visLeft = Math.max(rect.left, clip.left, 0);
    const visTop = Math.max(rect.top, clip.top, 0);
    const visRight = Math.min(rect.right, clip.right, innerWidth);
    const visBottom = Math.min(rect.bottom, clip.bottom, innerHeight);
    if (visRight - visLeft < 2 || visBottom - visTop < 4) return false;
    const x = Math.min(Math.max(visLeft + Math.min((visRight - visLeft) * 0.5, 8), 0), Math.max(innerWidth - 1, 0));
    const y = Math.min(Math.max((visTop + visBottom) * 0.5, 0), Math.max(innerHeight - 1, 0));
    const hit = deepElementFromPoint(x, y);
    if (!hit) return true;
    if (
      hit === startEl ||
      composedContains(startEl, hit) ||
      composedContains(hit, startEl) ||
      (endEl && (hit === endEl || composedContains(endEl, hit) || composedContains(hit, endEl)))
    ) {
      return true;
    }
    const control = startEl.closest ? startEl.closest(INTERACTIVE_ANCESTORS) : null;
    if (control && composedContains(control, hit)) return true;
    return !coveredBy(hit, startEl, endEl);
  }

  function paragraphRect(rects) {
    if (!rects.length) return null;
    let { left, top, right, bottom } = rects[0];
    for (let i = 1; i < rects.length; i++) {
      const rect = rects[i];
      if (rect.top > bottom + Math.max(rect.height, 8)) break;
      left = Math.min(left, rect.left);
      top = Math.min(top, rect.top);
      right = Math.max(right, rect.right);
      bottom = Math.max(bottom, rect.bottom);
    }
    return { left, top, width: right - left, height: bottom - top, right, bottom };
  }

  // Every visible paragraph and word on screen (`VIS-04`, `FRM-01`).
  async function wordTargets() {
    const page = new PageText();
    const from = caretPoint() || center();
    const rangeOf = (start, end) => {
      const first = page.pointAt(start, "start");
      const last = page.pointAt(end, "end");
      if (!first || !last) return null;
      const range = document.createRange();
      range.setStart(first.node, first.offset);
      range.setEnd(last.node, last.offset);
      return range;
    };
    const clips = new Map();
    const paragraphs = [];
    const words = [];
    const offered = new Map();
    let lineStart = 0;
    for (const line of page.text.split("\n")) {
      const start = lineStart;
      lineStart += line.length + 1;
      if (!line.trim()) continue;
      const lineRange = rangeOf(start, start + line.length);
      const lineRect = lineRange ? lineRange.getBoundingClientRect() : null;
      if (!lineRect || !onScreen(lineRect)) continue;
      const shown = [];
      WORD.lastIndex = 0;
      for (const match of line.matchAll(WORD)) {
        const range = rangeOf(start + match.index, start + match.index + match[0].length);
        const rect = firstRangeRect(range);
        if (!rect || !onScreen(rect) || !wordShows(range, rect, clips)) continue;
        const clientRects = range.getClientRects();
        if (clientRects.length) {
          for (let i = 0; i < clientRects.length; i++) {
            const cr = clientRects[i];
            if (cr.width > 0 && cr.height > 0 && onScreen(cr)) shown.push(cr);
          }
        } else {
          shown.push(rect);
        }
        const wid = "w" + (start + match.index);
        offered.set(wid, { caret: { node: range.startContainer, offset: range.startOffset } });
        words.push(target(wid, rect, "word", { distance: distance(rect, from) }));
      }
      const pRect = paragraphRect(shown);
      if (!pRect) continue;
      const pid = "p" + start;
      offered.set(pid, { paragraph: lineRange });
      paragraphs.push(target(pid, pRect, "paragraph", { distance: distance(pRect, from) }));
    }
    const frameBatches = await Promise.all(
      visibleFrames().map(async frame => ({
        frame,
        box: frameBox(frame),
        sub: await evalInFrame(frame, { op: "jump_scan", jump: "words", pattern: "" }),
      }))
    );
    for (const { frame, box, sub } of frameBatches) {
      if (!sub || !Array.isArray(sub.targets)) continue;
      const prefix = frameId(frame) + "/";
      for (const item of sub.targets) {
        const mapped = fromFrame(box, item);
        if (!onScreen(mapped) || !uncoveredInFrame(frame, box, mapped)) continue;
        const id = prefix + item.id;
        offered.set(id, { frame, childId: item.id, isVisual: true });
        const t = target(id, mapped, item.kind, { distance: distance(mapped, from) });
        if (item.kind === "paragraph") paragraphs.push(t);
        else words.push(t);
      }
    }
    pickable = offered;
    words.sort((a, b) => a.distance - b.distance);
    return [...paragraphs, ...words].slice(0, MAX_WORD_TARGETS);
  }

  // ---------------------------------------------------------------------------------------------------------------
  // The page's text as one string, a line per block
  // ---------------------------------------------------------------------------------------------------------------

  const MAX_TEXT_LENGTH = 2000000;

  class PageText {
    constructor(rootNode) {
      this.nodes = [];
      this.bases = [];
      this.index = new Map();
      const blocks = new Map();
      const blockOf = element => {
        if (!blocks.has(element)) {
          const display = getComputedStyle(element).display;
          const inline = display.startsWith("inline") || display === "contents";
          const parent = composedParent(element);
          blocks.set(element, inline && parent ? blockOf(parent) : element);
        }
        return blocks.get(element);
      };
      const visible = new Map();
      const ancVisible = new Map();
      let previousBlock = null;
      const parts = [];
      let length = 0;
      const walkRoot = rootNode
        ? rootNode === document
          ? document.body || document.documentElement
          : rootNode
        : document.body || document.documentElement;
      const iter = rootNode ? rootTextNodes(walkRoot) : deepTextNodes(walkRoot);
      for (const node of iter) {
        const parent = composedParent(node);
        if (!node.data || !parent || !isTextRendered(parent, visible, ancVisible) || !node.data.trim()) continue;
        const block = blockOf(parent);
        if (previousBlock && block !== previousBlock) {
          parts.push("\n");
          length++;
        }
        previousBlock = block;
        this.index.set(node, this.nodes.length);
        this.nodes.push(node);
        this.bases.push(length);
        // Source whitespace shows as a space; the length stays, so offsets stay the nodes' own.
        parts.push(node.data.replace(/[\n\r\t]/g, " "));
        length += node.data.length;
        if (length > MAX_TEXT_LENGTH) break;
      }
      this.text = parts.join("");
    }

    // The first node index for which `test` holds, given it holds for every node after it.
    firstIndex(test) {
      let low = 0;
      let high = this.nodes.length;
      while (low < high) {
        const middle = (low + high) >> 1;
        if (test(this.nodes[middle], middle)) high = middle;
        else low = middle + 1;
      }
      return low;
    }

    offsetOf(node, offset) {
      const index = this.index.get(node);
      if (index !== undefined) return this.bases[index] + Math.min(offset, node.data.length);
      const point = document.createRange();
      point.setStart(node, offset);
      point.collapse(true);
      const next = this.firstIndex(text => {
        try {
          return point.comparePoint(text, 0) >= 0;
        } catch (_) {
          return false;
        }
      });
      return next === this.nodes.length ? this.text.length : this.bases[next];
    }

    // A position between two blocks belongs to the block after it for a start, the one before it for an end.
    pointAt(offset, side) {
      if (!this.nodes.length) return null;
      let index = this.firstIndex((_, i) => this.bases[i] > offset) - 1;
      index = Math.max(index, 0);
      const node = this.nodes[index];
      const inside = offset - this.bases[index];
      if (inside <= node.data.length) {
        if (side === "end" && inside === 0 && index > 0) {
          const prev = this.nodes[index - 1];
          if (this.bases[index - 1] + prev.data.length === offset) {
            return { node: prev, offset: prev.data.length };
          }
        }
        if (side === "start" && inside === node.data.length && this.nodes[index + 1]) {
          return { node: this.nodes[index + 1], offset: 0 };
        }
        return { node, offset: Math.max(inside, 0) };
      }
      return side === "start" && this.nodes[index + 1]
        ? { node: this.nodes[index + 1], offset: 0 }
        : { node, offset: node.data.length };
    }

    elementRange(element) {
      let first = -1;
      let last = -1;
      for (let i = 0; i < this.nodes.length; i++) {
        if (composedContains(element, this.nodes[i])) {
          if (first === -1) first = i;
          last = i;
        } else if (first !== -1) {
          break;
        }
      }
      if (first === -1) return null;
      return { start: this.bases[first], end: this.bases[last] + this.nodes[last].data.length };
    }
  }

  // ---------------------------------------------------------------------------------------------------------------
  // Text objects: `a` or `i` with brackets, quotes, words, sentences, paragraphs and separators
  // ---------------------------------------------------------------------------------------------------------------

  const SEARCH_LINES = 50;
  const BRACKETS = {
    "(": ["(", ")"],
    ")": ["(", ")"],
    b: ["(", ")"],
    "[": ["[", "]"],
    "]": ["[", "]"],
    "{": ["{", "}"],
    "}": ["{", "}"],
    B: ["{", "}"],
    "<": ["<", ">"],
    ">": ["<", ">"],
  };
  // Quotes close with themselves, or with their curly twin.
  const QUOTES = {
    '"': ['"', "“", "”"],
    "'": ["'", "‘", "’"],
    "`": ["`", "`", "`"],
  };
  const SEPARATORS = "_.,;:+-=~|/\\&$#";
  const OBJECT_KEYS = new Set([
    ...Object.keys(BRACKETS),
    ...Object.keys(QUOTES),
    "q",
    "w",
    "W",
    "e",
    "d",
    "s",
    "p",
    "t",
    "g",
    ...SEPARATORS,
  ]);

  function unicodeRegex(source, fallback) {
    try {
      return new RegExp(source, "gu");
    } catch (_) {
      return fallback;
    }
  }
  const OBJECT_WORD = unicodeRegex("[\\p{L}\\p{N}_]+", /[A-Za-z0-9_À-￿]+/g);
  const NON_BLANK = /\S+/g;
  const SUBWORD = unicodeRegex("[\\p{Lu}]?[\\p{Ll}]+|[\\p{Lu}]+(?![\\p{Ll}])|\\p{N}+", /[A-Z]?[a-z]+|[A-Z]+(?![a-z])|\d+/g);
  const DIGITS = /\d+/g;
  const SENTENCE = /[^.!?]*[^\s.!?][.!?]*\s*/g;
  const LETTER = unicodeRegex("\\p{L}", /[A-Za-zÀ-￿]/g);

  function isLetter(char) {
    LETTER.lastIndex = 0;
    return !!char && LETTER.test(char);
  }

  function lineStarts(text) {
    const starts = [0];
    for (let i = text.indexOf("\n"); i !== -1; i = text.indexOf("\n", i + 1)) starts.push(i + 1);
    return starts;
  }

  function lines(text) {
    const starts = lineStarts(text);
    return starts.map((start, i) => ({
      start,
      text: text.slice(start, (starts[i + 1] === undefined ? text.length + 1 : starts[i + 1]) - 1),
    }));
  }

  function spansPerLine(text, pattern, toRange) {
    const found = [];
    for (const line of lines(text)) {
      pattern.lastIndex = 0;
      for (const match of line.text.matchAll(pattern)) found.push(toRange(match, line.start));
    }
    return found;
  }

  const span = (match, start) => ({ start: start + match.index, end: start + match.index + match[0].length });

  function bracketPairs(text, open, close) {
    const pairs = [];
    const stack = [];
    for (let i = 0; i < text.length; i++) {
      if (text[i] === open) stack.push(i);
      else if (text[i] === close && stack.length) pairs.push({ open: stack.pop(), close: i });
    }
    return pairs;
  }

  function quotePairs(text, kind) {
    const [straight, opening, closing] = QUOTES[kind];
    const pairs = [];
    for (const line of lines(text)) {
      let open = null;
      for (let i = 0; i < line.text.length; i++) {
        const char = line.text[i];
        if (char === straight) {
          // The apostrophe of "don't" is no quote.
          if (kind === "'" && isLetter(line.text[i - 1]) && isLetter(line.text[i + 1])) continue;
          if (open === null) {
            open = i;
          } else {
            pairs.push({ open: line.start + open, close: line.start + i });
            open = null;
          }
        } else if (char === opening) {
          open = i;
        } else if (char === closing && open !== null) {
          pairs.push({ open: line.start + open, close: line.start + i });
          open = null;
        }
      }
    }
    return pairs;
  }

  // `a` takes the blanks after a word, or the ones before it at the end of a line.
  function withTrailing(text, ranges, inner) {
    if (inner) return ranges;
    return ranges.map(range => {
      let end = range.end;
      while (text[end] === " ") end++;
      if (end > range.end) return { start: range.start, end };
      let start = range.start;
      while (text[start - 1] === " ") start--;
      return { start, end: range.end };
    });
  }

  function candidates(text, key, kind) {
    const inner = kind === "i";
    const pairRanges = pairs =>
      pairs
        .map(({ open, close }) => (inner ? { start: open + 1, end: close } : { start: open, end: close + 1 }))
        .filter(range => range.end > range.start);
    if (key === "b") {
      return pairRanges([
        ...bracketPairs(text, "(", ")"),
        ...bracketPairs(text, "[", "]"),
        ...bracketPairs(text, "{", "}"),
      ]);
    }
    if (BRACKETS[key]) return pairRanges(bracketPairs(text, BRACKETS[key][0], BRACKETS[key][1]));
    if (QUOTES[key]) return pairRanges(quotePairs(text, key));
    if (key === "q") return Object.keys(QUOTES).flatMap(each => pairRanges(quotePairs(text, each)));
    if (SEPARATORS.includes(key)) {
      // Between two of the character on a line; `a` takes the right one too.
      return lines(text).flatMap(line => {
        const found = [];
        let previous = -1;
        for (let i = 0; i < line.text.length; i++) {
          if (line.text[i] !== key) continue;
          if (previous !== -1) found.push({ start: line.start + previous + 1, end: line.start + i + (inner ? 0 : 1) });
          previous = i;
        }
        return found.filter(range => range.end > range.start);
      });
    }
    switch (key) {
      case "w":
        return withTrailing(text, spansPerLine(text, OBJECT_WORD, span), inner);
      case "W":
        return withTrailing(text, spansPerLine(text, NON_BLANK, span), inner);
      case "e":
        // A part of a name, with the `_` or `-` that joins it to the next.
        return spansPerLine(text, SUBWORD, span).map(range =>
          inner || !"_-".includes(text[range.end] || "x") ? range : { start: range.start, end: range.end + 1 }
        );
      case "d":
        return spansPerLine(text, DIGITS, span);
      case "s":
        return spansPerLine(text, SENTENCE, (match, start) => ({
          start: start + match.index,
          end: start + match.index + (inner ? match[0].trimEnd().length : match[0].length),
        }));
      case "p":
        return lines(text)
          .map(line => ({ start: line.start, end: line.start + line.text.length }))
          .filter(range => range.end > range.start);
      case "g":
        return [
          inner ? { start: text.length - text.trimStart().length, end: text.trimEnd().length } : { start: 0, end: text.length },
        ].filter(range => range.end > range.start);
    }
    return [];
  }

  // For a caret, the smallest candidate that covers it, else the next one; for a selection, the smallest that holds
  // it and is bigger. `next` (`an`/`in`) walks forward to the next object; `last` (`al`/`il`) walks backward.
  function chooseRange(ranges, selection, scope) {
    const size = range => range.end - range.start;
    if (scope === "next") {
      const collapsed = selection.start === selection.end;
      const after = ranges
        .filter(range => (collapsed ? range.start > selection.start : range.start >= selection.end))
        .sort((a, b) => a.start - b.start || size(a) - size(b));
      return after[0] || null;
    }
    if (scope === "last") {
      const before = ranges
        .filter(range => range.end <= selection.start && (range.start !== selection.start || range.end !== selection.end))
        .sort((a, b) => b.end - a.end || size(a) - size(b));
      if (before.length) return before[0];
      const earlier = ranges
        .filter(range => range.start < selection.start)
        .sort((a, b) => b.start - a.start || size(a) - size(b));
      return earlier[0] || null;
    }
    const collapsed = selection.start === selection.end;
    const containing = ranges
      .filter(range =>
        collapsed
          ? range.start <= selection.start && selection.start < range.end
          : range.start <= selection.start && selection.end <= range.end && size(range) > selection.end - selection.start
      )
      .sort((a, b) => size(a) - size(b));
    if (containing.length) return containing[0];
    if (!collapsed) return null;
    const next = ranges
      .filter(range => range.start >= selection.start)
      .sort((a, b) => a.start - b.start || size(a) - size(b));
    return next[0] || null;
  }

  function findTextObject(text, kind, key, selection, times, scope) {
    if (!OBJECT_KEYS.has(key) || (kind !== "a" && kind !== "i")) return null;
    // Only the lines near the selection: an unmatched bracket would otherwise pair with one a page away.
    const starts = lineStarts(text);
    const lineOf = offset => {
      let line = 0;
      while (line + 1 < starts.length && starts[line + 1] <= offset) line++;
      return line;
    };
    const first = Math.max(0, lineOf(selection.start) - SEARCH_LINES);
    const last = Math.min(starts.length - 1, lineOf(selection.end) + SEARCH_LINES);
    const from = key === "g" ? 0 : starts[first];
    const to = key === "g" || starts[last + 1] === undefined ? text.length : starts[last + 1];
    const slice = text.slice(from, to);
    let current = { start: selection.start - from, end: selection.end - from };
    let result = null;
    const all = candidates(slice, key, kind);
    for (let i = 0; i < times; i++) {
      const range = chooseRange(all, current, scope || "cover_or_next");
      if (!range) break;
      result = range;
      current = range;
    }
    return result && { start: result.start + from, end: result.end + from };
  }

  // Where a paragraph motion goes: the end of the paragraph it is in (or the start going back), and from there the
  // next one's.
  function paragraphEdge(text, offset, forward) {
    const start = offset > 0 ? text.lastIndexOf("\n", offset - 1) + 1 : 0;
    let end = text.indexOf("\n", offset);
    if (end === -1) end = text.length;
    if (forward) {
      if (offset < end) return { offset: end, side: "end" };
      const next = text.indexOf("\n", end + 1);
      return end >= text.length
        ? { offset: text.length, side: "end" }
        : { offset: next === -1 ? text.length : next, side: "end" };
    }
    if (offset > start) return { offset: start, side: "start" };
    return start === 0 ? { offset: 0, side: "start" } : { offset: text.lastIndexOf("\n", start - 2) + 1, side: "start" };
  }

  // ---------------------------------------------------------------------------------------------------------------
  // Scrolling the right element
  // ---------------------------------------------------------------------------------------------------------------

  const SCROLLABLE_OVERFLOW = new Set(["auto", "scroll", "overlay"]);
  let largestScroller = null;

  function canScroll(element, vertical) {
    const room = vertical ? element.scrollHeight - element.clientHeight : element.scrollWidth - element.clientWidth;
    if (room <= 1) return false;
    if (element === document.scrollingElement) return true;
    const style = getComputedStyle(element);
    return SCROLLABLE_OVERFLOW.has(vertical ? style.overflowY : style.overflowX);
  }

  // The element a page scrolls in when its document does not, like a web app's main pane. Found once per document.
  function largestScrollable(vertical) {
    const cached = largestScroller && largestScroller.deref ? largestScroller.deref() : largestScroller;
    if (cached && cached.isConnected && canScroll(cached, vertical)) return cached;
    let best = null;
    let bestArea = 0;
    if (document.body) {
      for (const element of deepElements(document.body)) {
        const area = element.clientWidth * element.clientHeight;
        if (area > bestArea && canScroll(element, vertical)) {
          best = element;
          bestArea = area;
        }
      }
    }
    if (best) largestScroller = typeof WeakRef === "function" ? new WeakRef(best) : best;
    return best;
  }

  // What the focused element scrolls in, else the document, else the largest scroller of the page.
  function scrollTarget(vertical) {
    let element = deepActiveElement();
    while (element && element !== document.body) {
      if (element.nodeType === 1 && canScroll(element, vertical)) return element;
      element = composedParent(element);
    }
    const root = document.scrollingElement;
    if (root && canScroll(root, vertical)) return root;
    return largestScrollable(vertical) || root;
  }

  function viewportSize(target, vertical) {
    if (target === document.scrollingElement) return vertical ? innerHeight : innerWidth;
    return vertical ? target.clientHeight : target.clientWidth;
  }

  async function scroll(request) {
    const activeEl = deepActiveElement();
    if (activeEl && (activeEl.localName === "iframe" || activeEl.localName === "frame")) {
      const sub = await evalInFrame(activeEl, Object.assign({ op: "scroll_if_able" }, request));
      if (sub && sub.scrolled) return { kind: "ok" };
    }
    const step = 60;
    const count = Math.max(1, request.count || 1);
    const behavior = request.smooth === false ? "instant" : "smooth";
    const motion = request.motion;
    if (motion === "top" || motion === "bottom") {
      const target = scrollTarget(true);
      if (target) target.scrollTo({ top: motion === "bottom" ? target.scrollHeight : 0, behavior });
      return { kind: "ok" };
    }
    const vertical = motion !== "left" && motion !== "right";
    const target = scrollTarget(vertical);
    if (!target) return { kind: "ok" };
    const page = fraction => viewportSize(target, true) * fraction * count;
    const amounts = {
      down: step * count,
      up: -step * count,
      right: step * count,
      left: -step * count,
      half_page_down: page(0.5),
      half_page_up: -page(0.5),
      page_down: page(0.9),
      page_up: -page(0.9),
    };
    const amount = amounts[motion] || 0;
    target.scrollBy(vertical ? { top: amount, behavior } : { left: amount, behavior });
    return { kind: "ok" };
  }

  async function scrollIfAble(request) {
    const motion = request.motion;
    const vertical = motion !== "left" && motion !== "right";
    const target = scrollTarget(vertical);
    if (!target || !canScroll(target, vertical)) return { kind: "scroll_result", scrolled: false };
    const before = vertical ? target.scrollTop : target.scrollLeft;
    await scroll(Object.assign({}, request, { smooth: false }));
    const after = vertical ? target.scrollTop : target.scrollLeft;
    return { kind: "scroll_result", scrolled: Math.abs(after - before) > 0.5 };
  }

  // ---------------------------------------------------------------------------------------------------------------
  // Visual mode
  // ---------------------------------------------------------------------------------------------------------------

  let activeVisualFrame = null;
  let visualPreferredX = null;

  function pageForSelection(selection) {
    const anchor = (selection && (selection.anchorNode || selection.focusNode)) || null;
    const root = anchor && typeof anchor.getRootNode === "function" ? anchor.getRootNode() : null;
    const page = new PageText(root);
    return page.nodes.length ? page : new PageText();
  }

  function extendSelectionTo(selection, node, offset) {
    if (typeof selection.setBaseAndExtent === "function" && selection.anchorNode) {
      selection.setBaseAndExtent(selection.anchorNode, selection.anchorOffset, node, offset);
    } else {
      selection.extend(node, offset);
    }
  }

  function focusRect() {
    const selection = getSelection();
    if (!selection || !selection.focusNode) return null;
    try {
      const range = document.createRange();
      range.setStart(selection.focusNode, selection.focusOffset);
      range.collapse(true);
      const rect =
        range.getClientRects()[0] ||
        (selection.focusNode.nodeType === 1 ? selection.focusNode.getBoundingClientRect() : null);
      return rect && rect.height ? rect : null;
    } catch (_) {
      return null;
    }
  }

  function caret() {
    const rect = focusRect();
    return rect ? { x: rect.left, y: rect.top, width: 2, height: rect.height } : null;
  }

  // Scroll every scrollable ancestor from `startElement` up through shadow roots and the document (`VIS-01`).
  function revealRectInAncestors(startElement, readRect) {
    const margin = 8;
    for (
      let anc = startElement;
      anc && anc !== document.body && anc !== document.documentElement;
      anc = composedParent(anc)
    ) {
      if (anc.nodeType !== 1) continue;
      const style = getComputedStyle(anc);
      if (SCROLLABLE_OVERFLOW.has(style.overflowY) && anc.scrollHeight > anc.clientHeight + 1) {
        const rect = readRect();
        if (rect) {
          const boxTop = anc.getBoundingClientRect().top + anc.clientTop;
          const boxBottom = boxTop + anc.clientHeight;
          if (rect.top < boxTop) anc.scrollBy({ top: rect.top - boxTop - margin, behavior: "instant" });
          else if (rect.bottom > boxBottom) anc.scrollBy({ top: rect.bottom - boxBottom + margin, behavior: "instant" });
        }
      }
      if (SCROLLABLE_OVERFLOW.has(style.overflowX) && anc.scrollWidth > anc.clientWidth + 1) {
        const rect = readRect();
        if (rect) {
          const boxLeft = anc.getBoundingClientRect().left + anc.clientLeft;
          const boxRight = boxLeft + anc.clientWidth;
          if (rect.left < boxLeft) anc.scrollBy({ left: rect.left - boxLeft - margin, behavior: "instant" });
          else if (rect.right > boxRight) anc.scrollBy({ left: rect.right - boxRight + margin, behavior: "instant" });
        }
      }
    }
    const rect = readRect();
    if (!rect) return;
    let dy = 0;
    if (rect.top < 0) dy = rect.top - margin;
    else if (rect.bottom > innerHeight) dy = rect.bottom - innerHeight + margin;
    let dx = 0;
    if (rect.left < 0) dx = rect.left - margin;
    else if (rect.right > innerWidth) dx = rect.right - innerWidth + margin;
    if (dy) {
      const root = document.scrollingElement;
      const target = root && canScroll(root, true) ? root : largestScrollable(true) || root;
      if (target) target.scrollBy({ top: dy, behavior: "instant" });
    }
    if (dx) {
      const root = document.scrollingElement;
      const target = root && canScroll(root, false) ? root : largestScrollable(false) || root;
      if (target) target.scrollBy({ left: dx, behavior: "instant" });
    }
  }

  function reveal() {
    const selection = getSelection();
    if (!selection || !selection.focusNode) return;
    const startEl = selection.focusNode.nodeType === 1 ? selection.focusNode : composedParent(selection.focusNode);
    revealRectInAncestors(startEl, focusRect);
  }

  function mapFrameVisualState(frame, sub, scrollFrameIntoView) {
    if (!sub || sub.kind !== "visual_state") return sub || visualState(false);
    if (scrollFrameIntoView && sub.caret) {
      revealRectInAncestors(frame, () => {
        const b = frameBox(frame);
        const m = fromFrame(b, sub.caret);
        return { top: m.top, bottom: m.bottom, left: m.left, right: m.right };
      });
    }
    const box = frameBox(frame);
    let mappedCaret = null;
    if (sub.caret) {
      const c = fromFrame(box, sub.caret);
      mappedCaret = { x: c.x, y: c.y, width: 2, height: c.height };
    }
    const mappedFlash = Array.isArray(sub.flash)
      ? sub.flash.map(r => {
          const m = fromFrame(box, r);
          return { x: m.x, y: m.y, width: m.width, height: m.height };
        })
      : [];
    if (!sub.active && activeVisualFrame === frame) activeVisualFrame = null;
    return Object.assign({}, sub, { caret: mappedCaret, flash: mappedFlash });
  }

  function visualState(active, extra) {
    return Object.assign(
      { kind: "visual_state", active, yanked: null, copied_by_page: false, caret: active ? caret() : null },
      extra
    );
  }

  function firstVisibleText() {
    const visible = new Map();
    const ancVisible = new Map();
    const clips = new Map();
    for (const node of deepTextNodes(document.body || document.documentElement)) {
      const parent = composedParent(node);
      if (!node.data.trim() || !parent || !isTextRendered(parent, visible, ancVisible)) continue;
      const offset = node.data.search(/\S/);
      const range = document.createRange();
      range.setStart(node, offset);
      range.setEnd(node, offset + 1);
      const rect = firstRangeRect(range);
      if (rect && onScreen(rect) && wordShows(range, rect, clips)) return { node, offset };
    }
    return null;
  }

  function focusOnScreen() {
    const rect = focusRect();
    return !!rect && rect.bottom > 0 && rect.top < innerHeight && rect.right > 0 && rect.left < innerWidth;
  }

  // `v`: the cursor stays where the selection is when it is on screen, else it starts at the first text on screen.
  // A collapsed cursor asks for labels on every word and paragraph, to say where to land.
  async function visualEnter() {
    if (activeVisualFrame && activeVisualFrame.isConnected && frameVisible(activeVisualFrame)) {
      const sub = await evalInFrame(activeVisualFrame, { op: "visual_enter" });
      if (sub && sub.active) return mapFrameVisualState(activeVisualFrame, sub, false);
      activeVisualFrame = null;
    }
    const selection = getSelection();
    if (selection && (!selection.rangeCount || !focusOnScreen())) {
      const start = firstVisibleText();
      if (start) {
        selection.collapse(start.node, start.offset);
      } else {
        for (const frame of visibleFrames()) {
          const sub = await evalInFrame(frame, { op: "visual_enter" });
          if (sub && sub.active) {
            activeVisualFrame = frame;
            return mapFrameVisualState(frame, sub, false);
          }
        }
        return visualState(false);
      }
    } else if (!selection) {
      return visualState(false);
    }
    visualPreferredX = null;
    return visualState(true, { land: selection.isCollapsed });
  }

  // The cursor stays where the selection ended.
  async function visualExit() {
    visualPreferredX = null;
    if (activeVisualFrame && activeVisualFrame.isConnected) {
      const frame = activeVisualFrame;
      activeVisualFrame = null;
      await evalInFrame(frame, { op: "visual_exit" });
    }
    const selection = getSelection();
    if (selection && selection.focusNode) selection.collapse(selection.focusNode, selection.focusOffset);
    return visualState(false);
  }

  async function visualSwapEnds() {
    visualPreferredX = null;
    if (activeVisualFrame && activeVisualFrame.isConnected) {
      return mapFrameVisualState(
        activeVisualFrame,
        await evalInFrame(activeVisualFrame, { op: "visual_swap_ends" }),
        true
      );
    }
    const selection = getSelection();
    if (selection && selection.anchorNode && selection.focusNode) {
      const { anchorNode, anchorOffset, focusNode, focusOffset } = selection;
      selection.setBaseAndExtent(focusNode, focusOffset, anchorNode, anchorOffset);
      reveal();
    }
    return visualState(true);
  }

  const VISUAL_WORD_CHAR = unicodeRegex("[\\p{L}\\p{N}_]", /[A-Za-z0-9_À-\u00ff]/);

  function visualCharClass(ch) {
    if (!ch || /\s/.test(ch)) return 0;
    VISUAL_WORD_CHAR.lastIndex = 0;
    return VISUAL_WORD_CHAR.test(ch) ? 1 : 2;
  }

  function nextCharEnd(text, offset) {
    if (offset >= text.length) return text.length;
    const cp = text.codePointAt(offset);
    return Math.min(text.length, offset + (cp > 0xffff ? 2 : 1));
  }

  function visualCharRect(page, offset, probeRange) {
    if (offset < 0 || offset >= page.text.length || page.text[offset] === "\n") return null;
    const index = page.firstIndex((_, i) => page.bases[i] > offset) - 1;
    if (index < 0) return null;
    const node = page.nodes[index];
    let local = offset - page.bases[index];
    if (local < 0 || local >= node.data.length) return null;
    let code = node.data.charCodeAt(local);
    if (code >= 0xdc00 && code <= 0xdfff && local > 0) {
      local--;
      code = node.data.charCodeAt(local);
    }
    const step = code >= 0xd800 && code <= 0xdbff && local + 1 < node.data.length ? 2 : 1;
    try {
      probeRange.setStart(node, local);
      probeRange.setEnd(node, Math.min(local + step, node.data.length));
      return firstRangeRect(probeRange);
    } catch (_) {
      return null;
    }
  }

  function isSameVisualLine(a, b) {
    if (!a || !b) return false;
    const overlap = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
    const minH = Math.min(a.height, b.height);
    if (minH > 0 && overlap >= minH * 0.35) return true;
    const midA = (a.top + a.bottom) * 0.5;
    const midB = (b.top + b.bottom) * 0.5;
    return Math.abs(midA - midB) <= Math.max(minH * 0.45, 4);
  }

  function sampleVisualOffset(page, offset, dir, limit, probeRange) {
    const text = page.text;
    const len = text.length;
    if (!len) return null;
    let pos = Math.max(0, Math.min(offset, len - 1));
    const end = Math.max(0, Math.min(limit, len - 1));
    while (dir > 0 ? pos <= end : pos >= end) {
      if (text[pos] === "\n") {
        pos += dir;
        continue;
      }
      const rect = visualCharRect(page, pos, probeRange);
      if (rect) return { offset: pos, rect };
      if (text[pos] !== " ") {
        const idx = page.firstIndex((_, i) => page.bases[i] > pos) - 1;
        if (idx >= 0) {
          pos = dir > 0 ? page.bases[idx] + page.nodes[idx].data.length : page.bases[idx] - 1;
          continue;
        }
      }
      pos += dir;
    }
    return null;
  }

  function findLineBreak(page, anchorOffset, lineRect, dir, probeRange) {
    const len = page.text.length;
    if (!len) return null;
    const bound = dir > 0 ? len - 1 : 0;
    let same = anchorOffset;
    let diff = null;
    let step = 1;
    let pos = anchorOffset;
    while (true) {
      if (dir > 0 ? pos >= bound : pos <= bound) return null;
      const candidate = dir > 0 ? Math.min(pos + step, bound) : Math.max(pos - step, bound);
      const vis = sampleVisualOffset(page, candidate, dir, bound, probeRange);
      if (!vis) return null;
      if (!isSameVisualLine(lineRect, vis.rect)) {
        diff = vis.offset;
        break;
      }
      same = vis.offset;
      pos = vis.offset;
      step = Math.min(step * 2, 128);
    }
    let lo = same;
    let hi = diff;
    while (Math.abs(hi - lo) > 1) {
      const mid = (lo + hi) >> 1;
      if (mid === lo || mid === hi) break;
      const vis = sampleVisualOffset(page, mid, dir, hi, probeRange);
      if (!vis || vis.offset === hi) {
        const visBack = sampleVisualOffset(page, mid, -dir, lo, probeRange);
        if (!visBack || visBack.offset === lo) break;
        if (isSameVisualLine(lineRect, visBack.rect)) lo = visBack.offset;
        else hi = visBack.offset;
        continue;
      }
      if (isSameVisualLine(lineRect, vis.rect)) lo = vis.offset;
      else hi = vis.offset;
    }
    return { sameEnd: lo, nextStart: hi };
  }

  function lineEndCaretOffset(page, lineStart, lineEnd, probeRange) {
    const text = page.text;
    let end = nextCharEnd(text, lineEnd);
    if (text[lineEnd] === " " && lineEnd > lineStart && end < text.length && text[end] !== "\n") {
      const endRect = visualCharRect(page, lineEnd, probeRange);
      const afterRect = visualCharRect(page, end, probeRange);
      if (endRect && afterRect && !isSameVisualLine(endRect, afterRect)) {
        end = lineEnd;
      }
    }
    return end;
  }

  function closestOffsetOnLine(page, lineStart, lineEnd, preferredX, probeRange) {
    const text = page.text;
    const startRect = visualCharRect(page, lineStart, probeRange);
    const endRect = visualCharRect(page, lineEnd, probeRange);
    if (!startRect || !endRect) return { offset: lineStart, side: "start" };
    const ltr = startRect.left <= endRect.left;
    if (ltr) {
      if (preferredX <= startRect.left) return { offset: lineStart, side: "start" };
      if (preferredX >= endRect.right) {
        return { offset: lineEndCaretOffset(page, lineStart, lineEnd, probeRange), side: "end" };
      }
      let lo = lineStart;
      let hi = lineEnd;
      while (hi - lo > 6) {
        const mid = (lo + hi) >> 1;
        const vis = sampleVisualOffset(page, mid, +1, hi, probeRange);
        if (!vis || vis.offset >= hi) break;
        if (vis.rect.left <= preferredX) lo = vis.offset;
        else hi = vis.offset;
      }
      let best = { offset: lo, side: "start", dist: Infinity };
      for (let i = lo; i <= hi; ) {
        const r = visualCharRect(page, i, probeRange);
        const nextI = nextCharEnd(text, i);
        if (r) {
          const dLeft = Math.abs(r.left - preferredX);
          if (dLeft < best.dist) best = { offset: i, side: "start", dist: dLeft };
          const rightOff = i === lineEnd ? lineEndCaretOffset(page, lineStart, lineEnd, probeRange) : nextI;
          const dRight = Math.abs(r.right - preferredX);
          if (dRight < best.dist) best = { offset: rightOff, side: "end", dist: dRight };
        }
        i = nextI;
      }
      return { offset: best.offset, side: best.side };
    }
    let best = { offset: lineStart, side: "start", dist: Math.abs(startRect.right - preferredX) };
    for (let i = lineStart, scanned = 0; i <= lineEnd && scanned < 256; scanned++) {
      const r = visualCharRect(page, i, probeRange);
      const nextI = nextCharEnd(text, i);
      if (r) {
        const dRight = Math.abs(r.right - preferredX);
        if (dRight < best.dist) best = { offset: i, side: "start", dist: dRight };
        const dLeft = Math.abs(r.left - preferredX);
        if (dLeft < best.dist) best = { offset: nextI, side: "end", dist: dLeft };
      }
      i = nextI;
    }
    return { offset: best.offset, side: best.side };
  }

  function resolveCurrentVisualPos(page, curOffset, probeRange) {
    const len = page.text.length;
    if (!len) return null;
    const caretRect = focusRect();
    if (caretRect) {
      const rAt = visualCharRect(page, curOffset, probeRange);
      if (rAt && isSameVisualLine(caretRect, rAt)) {
        return { offset: curOffset, rect: rAt, caretX: caretRect.left };
      }
      const rPrev = curOffset > 0 ? visualCharRect(page, curOffset - 1, probeRange) : null;
      if (rPrev && isSameVisualLine(caretRect, rPrev)) {
        return { offset: curOffset - 1, rect: rPrev, caretX: caretRect.left };
      }
    }
    const vis =
      sampleVisualOffset(page, Math.min(curOffset, len - 1), +1, len - 1, probeRange) ||
      sampleVisualOffset(page, Math.max(curOffset - 1, 0), -1, 0, probeRange);
    if (!vis) return null;
    return { offset: vis.offset, rect: vis.rect, caretX: caretRect ? caretRect.left : vis.rect.left };
  }

  function stepLogicalLine(text, offset, forward, times) {
    const starts = lineStarts(text);
    let line = 0;
    while (line + 1 < starts.length && starts[line + 1] <= offset) line++;
    const col = offset - starts[line];
    const targetLine = Math.max(0, Math.min(starts.length - 1, line + (forward ? times : -times)));
    const targetStart = starts[targetLine];
    const targetEnd = starts[targetLine + 1] === undefined ? text.length : starts[targetLine + 1] - 1;
    return { offset: Math.min(targetStart + col, targetEnd), side: forward ? "end" : "start" };
  }

  function moveVisualLine(page, curOffset, forward, times, probeRange) {
    const cur = resolveCurrentVisualPos(page, curOffset, probeRange);
    if (!cur) return stepLogicalLine(page.text, curOffset, forward, times);
    if (visualPreferredX === null) visualPreferredX = cur.caretX;
    let lineOffset = cur.offset;
    let lineRect = cur.rect;
    const dir = forward ? +1 : -1;
    const len = page.text.length;
    for (let i = 0; i < times; i++) {
      const step1 = findLineBreak(page, lineOffset, lineRect, dir, probeRange);
      if (!step1) {
        if (forward) return { offset: len, side: "end" };
        const firstVis = sampleVisualOffset(page, 0, +1, len - 1, probeRange);
        return { offset: firstVis ? firstVis.offset : 0, side: "start" };
      }
      const nextEdge = step1.nextStart;
      const targetRect = visualCharRect(page, nextEdge, probeRange);
      if (!targetRect) return { offset: nextEdge, side: forward ? "start" : "end" };
      const step2 = findLineBreak(page, nextEdge, targetRect, dir, probeRange);
      let targetStart;
      let targetEnd;
      if (forward) {
        targetStart = nextEdge;
        if (step2) {
          targetEnd = step2.sameEnd;
        } else {
          const lastVis = sampleVisualOffset(page, len - 1, -1, targetStart, probeRange);
          targetEnd = lastVis ? lastVis.offset : targetStart;
        }
      } else {
        targetEnd = nextEdge;
        if (step2) {
          targetStart = step2.sameEnd;
        } else {
          const firstVis = sampleVisualOffset(page, 0, +1, targetEnd, probeRange);
          targetStart = firstVis ? firstVis.offset : targetEnd;
        }
      }
      if (i === times - 1) {
        return closestOffsetOnLine(page, targetStart, targetEnd, visualPreferredX, probeRange);
      }
      lineOffset = forward ? targetEnd : targetStart;
      lineRect = targetRect;
    }
    return { offset: curOffset, side: forward ? "end" : "start" };
  }

  function visualLineEdge(page, curOffset, toEnd, probeRange) {
    const text = page.text;
    const len = text.length;
    const cur = resolveCurrentVisualPos(page, curOffset, probeRange);
    if (!cur) {
      if (toEnd) {
        const nl = text.indexOf("\n", curOffset);
        return { offset: nl === -1 ? len : nl, side: "end" };
      }
      return { offset: curOffset > 0 ? text.lastIndexOf("\n", curOffset - 1) + 1 : 0, side: "start" };
    }
    const dir = toEnd ? +1 : -1;
    const brk = findLineBreak(page, cur.offset, cur.rect, dir, probeRange);
    if (toEnd) {
      const endChar = brk ? brk.sameEnd : (sampleVisualOffset(page, len - 1, -1, cur.offset, probeRange) || cur).offset;
      const startBrk = findLineBreak(page, cur.offset, cur.rect, -1, probeRange);
      const startChar = startBrk ? startBrk.sameEnd : 0;
      return { offset: lineEndCaretOffset(page, startChar, endChar, probeRange), side: "end" };
    }
    const startChar = brk ? brk.sameEnd : (sampleVisualOffset(page, 0, +1, cur.offset, probeRange) || cur).offset;
    return { offset: startChar, side: "start" };
  }

  function stepVisualChar(page, offset, forward, probeRange) {
    const text = page.text;
    const len = text.length;
    if (forward) {
      if (offset >= len) return { offset: len, side: "end" };
      let pos = offset;
      if (text[pos] === "\n") {
        pos++;
        while (pos < len && text[pos] === " " && !visualCharRect(page, pos, probeRange)) pos++;
      }
      if (pos >= len) return { offset: len, side: "end" };
      pos = nextCharEnd(text, pos);
      while (pos < len && text[pos] === " " && !visualCharRect(page, pos, probeRange)) pos++;
      return { offset: pos, side: "end" };
    }
    if (offset <= 0) return { offset: 0, side: "start" };
    let pos = offset;
    while (pos > 0 && text[pos - 1] === " " && !visualCharRect(page, pos - 1, probeRange)) pos--;
    if (pos > 0 && text[pos - 1] === "\n") {
      pos--;
      while (pos > 0 && text[pos - 1] === " " && !visualCharRect(page, pos - 1, probeRange)) pos--;
    }
    if (pos <= 0) return { offset: 0, side: "start" };
    pos--;
    if (
      pos > 0 &&
      text.charCodeAt(pos) >= 0xdc00 &&
      text.charCodeAt(pos) <= 0xdfff &&
      text.charCodeAt(pos - 1) >= 0xd800 &&
      text.charCodeAt(pos - 1) <= 0xdbff
    ) {
      pos--;
    }
    return { offset: pos, side: "start" };
  }

  function stepVisualWord(text, offset, forward) {
    const len = text.length;
    let pos = offset;
    if (forward) {
      if (pos < len) {
        const cls = visualCharClass(text[pos]);
        if (cls !== 0) {
          while (pos < len && visualCharClass(text[pos]) === cls) pos++;
        }
        while (pos < len && visualCharClass(text[pos]) === 0) pos++;
      }
      return { offset: pos, side: pos < len ? "start" : "end" };
    }
    while (pos > 0 && visualCharClass(text[pos - 1]) === 0) pos--;
    if (pos > 0) {
      const cls = visualCharClass(text[pos - 1]);
      while (pos > 0 && visualCharClass(text[pos - 1]) === cls) pos--;
    }
    return { offset: pos, side: "start" };
  }

  async function visualMove(motion, count) {
    if (activeVisualFrame && activeVisualFrame.isConnected) {
      return mapFrameVisualState(
        activeVisualFrame,
        await evalInFrame(activeVisualFrame, { op: "visual_move", motion, count }),
        true
      );
    }
    const selection = getSelection();
    if (!selection || !selection.focusNode) return visualState(true);
    const times = Math.max(1, count || 1);
    if (motion !== "down" && motion !== "up") visualPreferredX = null;

    // Move over the entire page's rendered text within the selection's DOM root so non-editable pages support
    // full-page `h`/`j`/`k`/`l`/`w`/`b`/`0`/`$`/`{`/`}`/`gg`/`G` without stopping at a single text node or line.
    const page = pageForSelection(selection);
    if (!page.nodes.length) return visualState(true);
    const probeRange = document.createRange();
    let place = { offset: page.offsetOf(selection.focusNode, selection.focusOffset), side: "end" };

    switch (motion) {
      case "left":
      case "right":
        for (let i = 0; i < times; i++) place = stepVisualChar(page, place.offset, motion === "right", probeRange);
        break;
      case "down":
      case "up":
        place = moveVisualLine(page, place.offset, motion === "down", times, probeRange);
        break;
      case "word_forward":
      case "word_backward":
        for (let i = 0; i < times; i++) place = stepVisualWord(page.text, place.offset, motion === "word_forward");
        break;
      case "line_start":
        place = visualLineEdge(page, place.offset, false, probeRange);
        break;
      case "line_end":
        place = visualLineEdge(page, place.offset, true, probeRange);
        break;
      case "paragraph_forward":
      case "paragraph_backward":
        for (let i = 0; i < times; i++) place = paragraphEdge(page.text, place.offset, motion === "paragraph_forward");
        break;
      case "top": {
        const firstVis = sampleVisualOffset(page, 0, +1, page.text.length - 1, probeRange);
        place = { offset: firstVis ? firstVis.offset : 0, side: "start" };
        break;
      }
      case "bottom":
        place = { offset: page.text.length, side: "end" };
        break;
      default:
        return visualState(true);
    }

    const point = page.pointAt(place.offset, place.side);
    if (point) {
      extendSelectionTo(selection, point.node, point.offset);
      if (motion === "top" || motion === "bottom") {
        await scroll({ motion, count: 1, smooth: false });
        reveal();
      } else {
        reveal();
      }
    }
    return visualState(true);
  }

  function selectedRange(page, selection) {
    const anchor = page.offsetOf(selection.anchorNode, selection.anchorOffset);
    const focus = page.offsetOf(selection.focusNode, selection.focusOffset);
    return { start: Math.min(anchor, focus), end: Math.max(anchor, focus) };
  }

  // The elements around the selection, as text objects: `t`, where `at` and `it` are the same.
  function elementRanges(page, selection) {
    let element = selection.focusNode;
    if (element && element.nodeType !== 1) element = element.parentElement;
    const ranges = [];
    for (; element; element = element.parentElement) {
      const range = page.elementRange(element);
      const last = ranges[ranges.length - 1];
      if (range && (!last || last.start !== range.start || last.end !== range.end)) ranges.push(range);
    }
    return ranges;
  }

  function allElementRanges(page) {
    const root = document.body || document.documentElement;
    if (!root) return [];
    const seen = new Set();
    const ranges = [];
    let count = 0;
    for (const element of deepElements(root)) {
      if (++count > 500) break;
      if (isNotTextElement(element)) continue;
      const range = page.elementRange(element);
      if (!range || range.end <= range.start) continue;
      const sig = range.start + ":" + range.end;
      if (seen.has(sig)) continue;
      seen.add(sig);
      ranges.push(range);
    }
    return ranges;
  }

  function parseTextObjectSpec(keys) {
    const raw = String(keys || "");
    let kind = "";
    let scope = "cover_or_next";
    let rest = "";
    if (raw.startsWith("g[")) {
      kind = "left";
      rest = raw.slice(2);
    } else if (raw.startsWith("g]")) {
      kind = "right";
      rest = raw.slice(2);
    } else if ((raw.startsWith("an") || raw.startsWith("in")) && raw.length > 2) {
      kind = raw[0];
      scope = "next";
      rest = raw.slice(2);
    } else if ((raw.startsWith("al") || raw.startsWith("il")) && raw.length > 2) {
      kind = raw[0];
      scope = "last";
      rest = raw.slice(2);
    } else {
      kind = raw[0] || "";
      rest = raw.slice(1);
    }
    const key = { "<lt>": "<", "<Bar>": "|", "<Bslash>": "\\" }[rest] || rest;
    return { kind, scope, key };
  }

  async function visualObject(keys, count) {
    if (activeVisualFrame && activeVisualFrame.isConnected) {
      return mapFrameVisualState(
        activeVisualFrame,
        await evalInFrame(activeVisualFrame, { op: "visual_text_object", keys, count }),
        true
      );
    }
    const selection = getSelection();
    if (!selection || !selection.rangeCount) return visualState(true);
    visualPreferredX = null;
    const { kind, scope, key } = parseTextObjectSpec(keys);
    const searchKind = kind === "left" || kind === "right" ? "a" : kind;
    const times = Math.max(1, count || 1);
    const page = pageForSelection(selection);
    let current = selectedRange(page, selection);
    let range = null;
    if (key === "t") {
      const ranges = scope === "cover_or_next" ? elementRanges(page, selection) : allElementRanges(page);
      for (let i = 0; i < times; i++) {
        const grown = chooseRange(ranges, current, scope);
        if (!grown) {
          range = null;
          break;
        }
        range = grown;
        current = grown;
      }
    } else {
      range = findTextObject(page.text, searchKind, key, current, times, scope);
    }
    if (range) {
      if (kind === "left" || kind === "right") {
        const edge = kind === "left" ? page.pointAt(range.start, "start") : page.pointAt(range.end, "end");
        if (edge) {
          extendSelectionTo(selection, edge.node, edge.offset);
          reveal();
        }
      } else {
        const start = page.pointAt(range.start, "start");
        const end = page.pointAt(range.end, "end");
        if (start && end) {
          selection.setBaseAndExtent(start.node, start.offset, end.node, end.offset);
          reveal();
        }
      }
    }
    return visualState(true);
  }

  function performPageCopy() {
    if (typeof globalThis.__zendiaCopy === "function") {
      try {
        return !!globalThis.__zendiaCopy();
      } catch (_) {}
    }
    try {
      return !!document.execCommand("copy");
    } catch (_) {
      return false;
    }
  }

  function performPageCut() {
    if (typeof globalThis.__zendiaCut === "function") {
      try {
        return !!globalThis.__zendiaCut();
      } catch (_) {}
    }
    try {
      return !!document.execCommand("cut");
    } catch (_) {
      return false;
    }
  }

  async function copySelection() {
    const editFrame = await focusedEditableFrame();
    if (editFrame) {
      await evalInFrame(editFrame, { op: "copy_selection" });
      return { kind: "ok" };
    }
    const sel = getSelection();
    if (!sel || sel.isCollapsed) {
      for (const frame of visibleFrames()) {
        const res = await evalInFrame(frame, { op: "copy_selection_if_any" });
        if (res && res.copied) return { kind: "ok" };
      }
    }
    performPageCopy();
    return { kind: "ok" };
  }

  async function cutSelection() {
    const editFrame = await focusedEditableFrame();
    if (editFrame) {
      await evalInFrame(editFrame, { op: "cut_selection" });
      return { kind: "ok" };
    }
    for (const frame of visibleFrames()) {
      const res = await evalInFrame(frame, { op: "cut_selection_if_any" });
      if (res && res.cut) return { kind: "ok" };
    }
    performPageCut();
    return { kind: "ok" };
  }

  async function cutSelectionIfAny() {
    const active = deepActiveElement();
    if (active && isTextField(active) && active.selectionStart !== active.selectionEnd) {
      return { kind: "cut_result", cut: performPageCut() };
    }
    if (active && active.isContentEditable) {
      const sel = getSelection();
      if (sel && sel.rangeCount && !sel.isCollapsed) {
        return { kind: "cut_result", cut: performPageCut() };
      }
    }
    for (const frame of visibleFrames()) {
      const res = await evalInFrame(frame, { op: "cut_selection_if_any" });
      if (res && res.cut) return res;
    }
    return { kind: "cut_result", cut: false };
  }

  async function copySelectionIfAny() {
    const active = deepActiveElement();
    if (active && isTextField(active) && active.selectionStart !== active.selectionEnd) {
      return { kind: "copy_result", copied: performPageCopy() };
    }
    const sel = getSelection();
    if (sel && sel.rangeCount && !sel.isCollapsed) {
      return { kind: "copy_result", copied: performPageCopy() };
    }
    for (const frame of visibleFrames()) {
      const res = await evalInFrame(frame, { op: "copy_selection_if_any" });
      if (res && res.copied) return res;
    }
    return { kind: "copy_result", copied: false };
  }

  // `y`: copy as the page's own copy does (`text/html` + `text/plain`, `VIS-02`), flash, and collapse at the start.
  async function visualYank() {
    visualPreferredX = null;
    if (activeVisualFrame && activeVisualFrame.isConnected) {
      const frame = activeVisualFrame;
      activeVisualFrame = null;
      return mapFrameVisualState(frame, await evalInFrame(frame, { op: "visual_yank" }), false);
    }
    const active = deepActiveElement();
    if (active && active.localName === "input" && (active.type || "").toLowerCase() === "password") {
      return { kind: "visual_state", active: false, yanked: null, copied_by_page: false, caret: null, flash: [] };
    }
    const selection = getSelection();
    let yanked = null;
    let copied_by_page = false;
    let flash = [];
    if (selection && selection.rangeCount && !selection.isCollapsed) {
      yanked = selection.toString();
      const ranges = [];
      for (let i = 0; i < selection.rangeCount; i++) ranges.push(selection.getRangeAt(i).cloneRange());
      for (const range of ranges) for (const rect of range.getClientRects()) flash.push(plainRect(rect));
      copied_by_page = performPageCopy();
      selection.collapse(ranges[0].startContainer, ranges[0].startOffset);
    }
    return { kind: "visual_state", active: false, yanked, copied_by_page, caret: null, flash };
  }

  async function visualCaret() {
    if (activeVisualFrame && activeVisualFrame.isConnected) {
      return mapFrameVisualState(activeVisualFrame, await evalInFrame(activeVisualFrame, { op: "caret" }), false);
    }
    return visualState(true);
  }

  // A label pick on text: the selection grows to it in visual mode; otherwise visual mode starts there.
  function moveCaret(node, offset, extend) {
    visualPreferredX = null;
    const selection = getSelection();
    if (extend && selection.rangeCount) extendSelectionTo(selection, node, offset);
    else selection.collapse(node, offset);
    reveal();
    return visualState(true);
  }

  // ---------------------------------------------------------------------------------------------------------------
  // Fields
  // ---------------------------------------------------------------------------------------------------------------

  const ROW_TOLERANCE = 4;
  let lastField = null;
  let lastFieldFrame = null;
  let lastPut = null;
  let reportedEditing = null;
  let focusSyncScheduled = false;

  async function anyChildFrameEditing() {
    for (const frame of allFrames()) {
      const res = await evalInFrame(frame, { op: "has_editable_focus" });
      if (res && res.editing) return true;
    }
    return false;
  }

  async function focusedEditableFrame() {
    const active = deepActiveElement();
    if (active && (active.localName === "iframe" || active.localName === "frame")) {
      const res = await evalInFrame(active, { op: "has_editable_focus" });
      if (res && res.editing) return active;
    }
    for (const frame of allFrames()) {
      const res = await evalInFrame(frame, { op: "has_editable_focus" });
      if (res && res.editing) return frame;
    }
    return null;
  }

  async function rememberFocusedEditable() {
    const active = deepActiveElement();
    const editing = isEditable(active);
    if (editing && active) {
      lastField = typeof WeakRef === "function" ? new WeakRef(active) : active;
      lastFieldFrame = null;
    }
    if (editing !== reportedEditing) {
      const wasEditing = reportedEditing === true;
      reportedEditing = editing;
      if (editing) {
        postEvent({ kind: "editable_focus", editing: true });
      } else if ((wasEditing || window.top === window) && !(await anyChildFrameEditing())) {
        postEvent({ kind: "editable_focus", editing: false });
      }
    }
  }

  function scheduleFocusSync() {
    if (focusSyncScheduled) return;
    focusSyncScheduled = true;
    Promise.resolve().then(() => {
      focusSyncScheduled = false;
      rememberFocusedEditable();
    });
  }

  // Seed lastField if a field was already focused before the runtime loaded into this document.
  {
    const initialActive = deepActiveElement();
    if (isEditable(initialActive) && initialActive) {
      lastField = typeof WeakRef === "function" ? new WeakRef(initialActive) : initialActive;
    }
  }

  document.addEventListener(
    "focusin",
    event => {
      const element = event.composedPath ? event.composedPath()[0] : event.target;
      if (isEditable(element)) {
        lastField = typeof WeakRef === "function" ? new WeakRef(element) : element;
        lastFieldFrame = null;
      }
      scheduleFocusSync();
    },
    true
  );
  document.addEventListener("focusout", scheduleFocusSync, true);
  document.addEventListener(
    "load",
    event => {
      const t = event.target;
      if (t && t.nodeType === 1 && (t.localName === "iframe" || t.localName === "frame")) {
        evalInFrame(t, { op: "init" });
      }
    },
    true
  );

  // Throttle scroll, resize, and optional popup DOM mutation reports to one per animation frame (`JMP-04`, `SL-02`, `EXT-01`).
  let enableMutationWatch = () => {};
  {
    let moveRaf = 0;
    let moveScrolled = false;
    let mutationObserver = null;
    const scheduleMove = scrolled => {
      if (scrolled) moveScrolled = true;
      if (moveRaf) return;
      moveRaf = requestAnimationFrame(() => {
        const didScroll = moveScrolled;
        moveRaf = 0;
        moveScrolled = false;
        postEvent({ kind: "page_moved", scrolled: didScroll });
      });
    };
    window.addEventListener("scroll", () => scheduleMove(true), { capture: true, passive: true });
    window.addEventListener("resize", () => scheduleMove(false), { passive: true });
    enableMutationWatch = () => {
      if (mutationObserver || typeof MutationObserver !== "function" || !document.documentElement) return;
      mutationObserver = new MutationObserver(() => scheduleMove(false));
      mutationObserver.observe(document.documentElement, {
        subtree: true,
        childList: true,
        attributes: true,
        characterData: true,
      });
    };

    const onCopyOrCut = event => {
      const active = deepActiveElement();
      if (active && active.localName === "input" && (active.type || "").toLowerCase() === "password") {
        return;
      }
      let text = "";
      if (event.clipboardData && typeof event.clipboardData.getData === "function") {
        try {
          text = event.clipboardData.getData("text/plain") || "";
        } catch (_) {}
      }
      if (!text && active && (active.localName === "input" || active.localName === "textarea")) {
        try {
          if (typeof active.selectionStart === "number" && typeof active.selectionEnd === "number") {
            text = (active.value || "").slice(active.selectionStart, active.selectionEnd);
          }
        } catch (_) {}
      }
      if (!text) {
        const sel = getSelection();
        if (sel) text = sel.toString();
      }
      if (text && text.trim() && text.length <= 100000) {
        postEvent({ kind: "page_yank", text, host: location.hostname || "" });
      }
    };
    document.addEventListener("copy", onCopyOrCut, true);
    document.addEventListener("cut", onCopyOrCut, true);
  }

  function lastInput() {
    const element = lastField && lastField.deref ? lastField.deref() : lastField;
    return element && element.isConnected ? element : null;
  }

  function isShownField(element) {
    if (!isField(element) || !isRendered(element)) return false;
    const rect = element.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  }

  // Local fields in reading order.
  function fields() {
    return [...deepElements(document)]
      .filter(isShownField)
      .map(element => ({ element, rect: element.getBoundingClientRect() }))
      .sort((a, b) => (Math.abs(a.rect.top - b.rect.top) > ROW_TOLERANCE ? a.rect.top - b.rect.top : a.rect.left - b.rect.left))
      .map(item => item.element);
  }

  // All fields across this document and child frames in reading order (`FRM-01`).
  async function allFieldEntries() {
    const entries = fields().map((element, index) => ({
      frame: null,
      element,
      childIndex: index,
      rect: plainRect(element.getBoundingClientRect()),
      active: element === deepActiveElement(),
    }));
    const frameBatches = await Promise.all(
      allFrames()
        .filter(frame => frameVisible(frame))
        .map(async frame => ({
          frame,
          box: frameBox(frame),
          sub: await evalInFrame(frame, { op: "frame_fields" }),
        }))
    );
    for (const { frame, box, sub } of frameBatches) {
      if (!sub || !Array.isArray(sub.fields)) continue;
      for (const f of sub.fields) {
        const mapped = fromFrame(box, f.rect);
        entries.push({
          frame,
          element: null,
          childIndex: f.index,
          rect: { x: mapped.x, y: mapped.y, width: mapped.width, height: mapped.height },
          active: !!f.active,
        });
      }
    }
    return entries.sort((a, b) =>
      Math.abs(a.rect.y - b.rect.y) > ROW_TOLERANCE ? a.rect.y - b.rect.y : a.rect.x - b.rect.x
    );
  }

  function rectInViewport(rect) {
    return rect.y + rect.height > 0 && rect.x + rect.width > 0 && rect.y < innerHeight && rect.x < innerWidth;
  }

  // Continue typing after what is there, as `A` would in vim.
  function focusField(element) {
    element.focus();
    try {
      const end = element.value === undefined ? undefined : element.value.length;
      if (end !== undefined) element.setSelectionRange(end, end);
    } catch (_) {}
    return { kind: "activated", mode: "i", url: null, message: null };
  }

  async function focusFieldEntry(entry) {
    if (!entry.frame) {
      lastFieldFrame = null;
      return focusField(entry.element);
    }
    lastFieldFrame = entry.frame;
    try {
      entry.frame.focus();
    } catch (_) {}
    return (
      (await evalInFrame(entry.frame, { op: "frame_focus_field", index: entry.childIndex })) || {
        kind: "activated",
        mode: "i",
        url: null,
        message: null,
      }
    );
  }

  async function inputFirst(count) {
    const visible = (await allFieldEntries()).filter(entry => rectInViewport(entry.rect));
    // No field to type in: the keys go to the page, as with `I`.
    if (!visible.length) return { kind: "explicit_insert" };
    return focusFieldEntry(visible[Math.min(Math.max(count || 1, 1), visible.length) - 1]);
  }

  // `i`: a count says which field and one field needs no choosing; several are labeled.
  async function inputChoose(count) {
    if (count === null || count === undefined) {
      const choices = await controlTargets("only", "");
      if (choices.length > 1) return { kind: "targets", jump: "inputs", pattern: "", targets: choices };
      if (choices.length === 1) return pick(choices[0].id, false, false);
    }
    return inputFirst(count);
  }

  async function inputLast(count) {
    if (lastFieldFrame && lastFieldFrame.isConnected) {
      const sub = await evalInFrame(lastFieldFrame, { op: "frame_focus_last" });
      if (sub && sub.kind === "activated") return sub;
    }
    const last = lastInput();
    if (last && isEditable(last) && isRendered(last)) return focusField(last);
    const list = await allFieldEntries();
    if (list.length) return focusFieldEntry(list[Math.min(Math.max(count || 1, 1), list.length) - 1]);
    return { kind: "ok" };
  }

  async function inputStep(offset) {
    const list = await allFieldEntries();
    if (!list.length) return { kind: "ok" };
    const current = list.findIndex(entry => entry.active);
    let index;
    if (current === -1) {
      const shown = list.findIndex(entry => rectInViewport(entry.rect));
      const start = shown === -1 ? 0 : shown;
      index = offset > 0 ? start + offset - 1 : start + offset;
    } else {
      index = current + offset;
    }
    return focusFieldEntry(list[((index % list.length) + list.length) % list.length]);
  }

  // `<Esc>` in insert mode: the field gives up the focus, in child frames too (`FRM-01`).
  async function blurField() {
    const editFrame = await focusedEditableFrame();
    if (editFrame) {
      await evalInFrame(editFrame, { op: "blur_field" });
    }
    const active = deepActiveElement();
    if (isEditable(active)) active.blur();
    return { kind: "ok" };
  }

  // `<C-u>`: delete back to the start of the current line without deleting a preceding newline.
  async function editDeleteToLineStart() {
    const editFrame = await focusedEditableFrame();
    if (editFrame) return (await evalInFrame(editFrame, { op: "edit_delete_to_line_start" })) || { kind: "ok" };
    const field = deepActiveElement();
    if (!isEditable(field) || field.localName === "select") return { kind: "ok" };
    if (isTextField(field)) {
      try {
        if (typeof field.selectionStart === "number" && typeof field.selectionEnd === "number") {
          const start = field.selectionStart;
          const end = field.selectionEnd;
          if (start !== end) {
            document.execCommand("delete", false, null);
            return { kind: "ok" };
          }
          const value = field.value || "";
          const lineStart = start > 0 ? value.lastIndexOf("\n", start - 1) + 1 : 0;
          if (lineStart < start) {
            field.setSelectionRange(lineStart, start);
            document.execCommand("delete", false, null);
          }
          return { kind: "ok" };
        }
      } catch (_) {}
      if (field.value) {
        if (typeof field.select === "function") field.select();
        document.execCommand("delete", false, null);
      }
      return { kind: "ok" };
    }
    const sel = getSelection();
    if (sel && sel.rangeCount) {
      if (sel.isCollapsed && typeof sel.modify === "function") {
        sel.modify("extend", "backward", "lineboundary");
      }
      if (!sel.isCollapsed) {
        document.execCommand("delete", false, null);
      }
    }
    return { kind: "ok" };
  }

  async function editDeleteToLineEnd() {
    const editFrame = await focusedEditableFrame();
    if (editFrame) return (await evalInFrame(editFrame, { op: "edit_delete_to_line_end" })) || { kind: "ok" };
    const field = deepActiveElement();
    if (!isEditable(field) || field.localName === "select") return { kind: "ok" };
    if (isTextField(field)) {
      try {
        if (typeof field.selectionStart === "number" && typeof field.selectionEnd === "number") {
          const start = field.selectionStart;
          const end = field.selectionEnd;
          if (start !== end) {
            document.execCommand("delete", false, null);
            return { kind: "ok" };
          }
          const value = field.value || "";
          const nl = value.indexOf("\n", start);
          const lineEnd = nl === -1 ? value.length : nl;
          if (start < lineEnd) {
            field.setSelectionRange(start, lineEnd);
            document.execCommand("delete", false, null);
          }
          return { kind: "ok" };
        }
      } catch (_) {}
      return { kind: "ok" };
    }
    const sel = getSelection();
    if (sel && sel.rangeCount) {
      if (sel.isCollapsed && typeof sel.modify === "function") {
        sel.modify("extend", "forward", "lineboundary");
      }
      if (!sel.isCollapsed) {
        document.execCommand("delete", false, null);
      }
    }
    return { kind: "ok" };
  }

  async function editDeleteCharForward() {
    const editFrame = await focusedEditableFrame();
    if (editFrame) return (await evalInFrame(editFrame, { op: "edit_delete_char_forward" })) || { kind: "ok" };
    const field = deepActiveElement();
    if (!isEditable(field) || field.localName === "select") return { kind: "ok" };
    document.execCommand("forwardDelete", false, null);
    return { kind: "ok" };
  }

  function isTextField(element) {
    return !!element && (element.localName === "input" || element.localName === "textarea");
  }

  // Put a yank where the caret is, or over the put the caret still sits at the end of. The editor's own insert keeps
  // undo and fires input events, so the page sees typing.
  async function yankPut(text, index, replace) {
    const editFrame = await focusedEditableFrame();
    if (editFrame) {
      const sub = await evalInFrame(editFrame, { op: "yank_put", text, index, replace });
      if (sub && sub.kind === "put") {
        const box = frameBox(editFrame);
        const flash = Array.isArray(sub.flash)
          ? sub.flash.map(r => {
              const m = fromFrame(box, r);
              return { x: m.x, y: m.y, width: m.width, height: m.height };
            })
          : [];
        return { kind: "put", ok: !!sub.ok, text: sub.text, flash };
      }
    }
    const field = deepActiveElement();
    if (!isEditable(field) || field.localName === "select") return { kind: "put", ok: false, text };
    const last = replace ? cyclingPut() : null;
    if (last && isTextField(field)) field.setSelectionRange(last.end - last.length, last.end);
    document.execCommand("insertText", false, text);
    lastPut = isTextField(field) ? { field, end: field.selectionEnd, length: text.length, index } : null;
    const rect = field.getBoundingClientRect();
    return { kind: "put", ok: true, text, flash: [plainRect(rect)] };
  }

  function cyclingPut() {
    const field = deepActiveElement();
    return lastPut && lastPut.field === field && field.selectionStart === lastPut.end && field.selectionEnd === lastPut.end
      ? lastPut
      : null;
  }

  // Which yank `<A-p>` (older, step 1) or `<A-n>` (newer, step -1) puts: the next one from the put the caret sits
  // after, or the newest.
  async function yankCycle(step) {
    const editFrame = await focusedEditableFrame();
    if (editFrame) return (await evalInFrame(editFrame, { op: "yank_cycle", step })) || { kind: "ok" };
    const field = deepActiveElement();
    if (!isEditable(field) || field.localName === "select") return { kind: "ok" };
    const last = cyclingPut();
    return { kind: "cycle", index: last ? Math.max(last.index + step, 0) : 0 };
  }

  // ---------------------------------------------------------------------------------------------------------------
  // Picking a target
  // ---------------------------------------------------------------------------------------------------------------

  // A field takes the focus in insert mode; any other control is pressed by the browser where it is on top (`ctrl`
  // holds Ctrl on capital hints so page handlers run, `JMP-03`); text moves the cursor (`FRM-01`).
  async function pick(id, newTab, extend) {
    const entry = pickable.get(id);
    if (!entry) return { kind: "ok" };
    if (entry.frame) {
      if (!entry.frame.isConnected) return { kind: "ok" };
      if (entry.isVisual) {
        const sameFrame = activeVisualFrame === entry.frame;
        if (!sameFrame) {
          const sel = getSelection();
          if (sel) sel.removeAllRanges();
        }
        activeVisualFrame = entry.frame;
        const sub = await evalInFrame(entry.frame, {
          op: "jump_pick",
          id: entry.childId,
          new_tab: !!newTab,
          extend: !!(extend && sameFrame),
        });
        return mapFrameVisualState(entry.frame, sub, true);
      }
      const box = frameBox(entry.frame);
      const sub = await evalInFrame(entry.frame, {
        op: "jump_pick",
        id: entry.childId,
        new_tab: !!newTab,
        extend: !!extend,
      });
      if (!sub) return { kind: "ok" };
      if (sub.kind === "press") {
        return {
          kind: "press",
          x: box.left + sub.x * box.scaleX,
          y: box.top + sub.y * box.scaleY,
          ctrl: !!sub.ctrl,
        };
      }
      if (sub.kind === "activated" && sub.mode === "i") {
        lastFieldFrame = entry.frame;
        try {
          entry.frame.focus();
        } catch (_) {}
      }
      return sub;
    }
    if (entry.control) {
      const element = entry.control;
      if (!element.isConnected) return { kind: "ok" };
      if (isEditable(element)) {
        lastFieldFrame = null;
        return focusField(element);
      }
      const point = pointOf(element);
      return { kind: "press", x: point.x, y: point.y, ctrl: !!newTab };
    }
    activeVisualFrame = null;
    if (entry.paragraph) {
      visualPreferredX = null;
      const range = entry.paragraph;
      getSelection().setBaseAndExtent(range.startContainer, range.startOffset, range.endContainer, range.endOffset);
      reveal();
      return visualState(true);
    }
    if (entry.caret) return moveCaret(entry.caret.node, entry.caret.offset, extend);
    return { kind: "ok" };
  }

  // ---------------------------------------------------------------------------------------------------------------
  // CAPTCHA actions, fill, touch drag & top-window refocus (`CAP-05`..`CAP-07`)
  // ---------------------------------------------------------------------------------------------------------------

  function refocusTop() {
    try {
      const focused = document.activeElement;
      if (focused && (focused.localName === "iframe" || focused.localName === "frame")) {
        focused.blur();
      }
      window.focus();
    } catch (_) {}
    return { kind: "ok" };
  }

  async function captchaAction(action) {
    const purposes = action === "audio" ? ["audio", "visual"] : [action];
    const targets = await controlTargets("include", "", "captcha");
    let chosen = null;
    for (const p of purposes) {
      chosen = targets.find(each => each.purpose === p);
      if (chosen) break;
    }
    if (!chosen) return { kind: "ok" };
    const result = await pick(chosen.id, false, false);
    if (result && result.kind === "press") {
      refocusTop();
    }
    return result;
  }

  function isTypeableField(element) {
    if (!element) return false;
    if (element.localName === "textarea" || element.isContentEditable) return true;
    return (
      element.localName === "input" &&
      !/^(checkbox|radio|button|submit|image|file|range|color|hidden)$/i.test(element.type || "text")
    );
  }

  async function captchaFill(text) {
    const val = String(text || "");
    const targets = await controlTargets("include", "", "captcha");
    const localOf = item => {
      const entry = pickable.get(item.id);
      return entry && entry.control ? entry.control : null;
    };
    const match =
      targets.find(each => each.purpose === "answer" && isTypeableField(localOf(each))) ||
      targets.find(each => isTypeableField(localOf(each)));
    const element = match ? localOf(match) : null;
    if (!element) {
      const remoteMatch = targets.find(each => each.purpose === "answer" && pickable.get(each.id)?.frame);
      if (remoteMatch) {
        const entry = pickable.get(remoteMatch.id);
        if (entry && entry.frame && entry.frame.isConnected) {
          return (await evalInFrame(entry.frame, { op: "captcha_fill", text: val })) || { kind: "ok" };
        }
      }
      return { kind: "ok" };
    }
    element.focus();
    if (element.localName === "input" || element.localName === "textarea") {
      try {
        const proto = element.localName === "textarea" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
        const desc = Object.getOwnPropertyDescriptor(proto, "value");
        if (desc && typeof desc.set === "function") desc.set.call(element, val);
        else element.value = val;
      } catch (_) {
        element.value = val;
      }
    } else {
      const sel = getSelection();
      if (sel) sel.selectAllChildren(element);
      document.execCommand("insertText", false, val);
    }
    element.dispatchEvent(new Event("input", { bubbles: true }));
    element.dispatchEvent(new Event("change", { bubbles: true }));
    return { kind: "ok" };
  }

  let activeTouchFrame = null;
  let activeTouchTarget = null;

  async function dispatchTouch(stage, x, y, resetChain) {
    if (resetChain) {
      activeTouchFrame = null;
      activeTouchTarget = null;
    }
    const frames = visibleFrames();
    let inner = null;
    if (stage !== "start" && activeTouchFrame) {
      inner = activeTouchFrame.isConnected ? activeTouchFrame : null;
    } else if (stage === "start") {
      for (let i = frames.length - 1; i >= 0; i--) {
        const box = frameBox(frames[i]);
        if (x >= box.left && x < box.right && y >= box.top && y < box.bottom) {
          inner = frames[i];
          break;
        }
      }
      activeTouchFrame = inner;
    }
    if (inner) {
      const box = frameBox(inner);
      await evalInFrame(inner, {
        op: "dispatch_touch",
        stage,
        x: (x - box.left) / box.scaleX,
        y: (y - box.top) / box.scaleY,
        reset_chain: !!resetChain,
      });
      if (stage === "end") activeTouchFrame = null;
      return { kind: "ok" };
    }
    const typeMap = { start: "touchstart", move: "touchmove", end: "touchend" };
    const eventType = typeMap[stage];
    if (!eventType) return { kind: "ok" };
    const el =
      (stage === "start" ? deepElementFromPoint(x, y) : activeTouchTarget) ||
      deepElementFromPoint(x, y) ||
      document.body ||
      document.documentElement;
    if (stage === "start") activeTouchTarget = el;
    if (stage === "end") activeTouchTarget = null;
    if (!el) return { kind: "ok" };
    try {
      if (typeof Touch === "function" && typeof TouchEvent === "function") {
        const touchObj = new Touch({
          identifier: 0,
          target: el,
          clientX: x,
          clientY: y,
          pageX: x + scrollX,
          pageY: y + scrollY,
          screenX: x,
          screenY: y,
        });
        const list = stage === "end" ? [] : [touchObj];
        el.dispatchEvent(
          new TouchEvent(eventType, {
            bubbles: true,
            cancelable: true,
            composed: true,
            touches: list,
            targetTouches: list,
            changedTouches: [touchObj],
          })
        );
        return { kind: "ok" };
      }
    } catch (_) {}
    try {
      const ev = new UIEvent(eventType, { bubbles: true, cancelable: true, composed: true });
      const pt = { identifier: 0, target: el, clientX: x, clientY: y, pageX: x + scrollX, pageY: y + scrollY };
      ev.touches = stage === "end" ? [] : [pt];
      ev.targetTouches = stage === "end" ? [] : [pt];
      ev.changedTouches = [pt];
      el.dispatchEvent(ev);
    } catch (_) {}
    return { kind: "ok" };
  }

  // ---------------------------------------------------------------------------------------------------------------
  // Requests
  // ---------------------------------------------------------------------------------------------------------------

  async function jumpTargets(request) {
    const pattern = request.pattern || "";
    const topOnly = !!request.top_only;
    switch (request.jump) {
      case "hints": {
        const items = await controlTargets(request.include_fields ? "include" : "exclude", "", "all", topOnly);
        if (topOnly) {
          items.sort((a, b) => (Math.abs(a.y - b.y) > ROW_TOLERANCE ? a.y - b.y : a.x - b.x));
        }
        return items;
      }
      case "controls":
        return controlTargets("include", pattern, "all", topOnly);
      case "inputs":
        return controlTargets("only", "", "all", topOnly);
      case "text":
        return pattern ? textTargets(pattern) : [];
      case "words":
      case "land":
        return wordTargets();
      case "captcha":
        return controlTargets("include", "", "captcha", topOnly);
      case "challenge":
        return controlTargets("include", "", "challenge", topOnly);
    }
    return [];
  }

  globalThis.__zendiaAutomation = async function (requestJson) {
    try {
      const request = JSON.parse(requestJson);
      let response;
      switch (request.op) {
        case "init":
          if (request.watch_mutations) enableMutationWatch();
          watchStart(true);
          await initChildFrames(false);
          await rememberFocusedEditable();
          response = { kind: "ok" };
          break;
        case "captcha_watch_scan":
          watchStart(true);
          await Promise.all(allFrames().map(frame => evalInFrame(frame, { op: "captcha_watch_scan" })));
          response = { kind: "ok" };
          break;
        case "has_editable_focus":
          response = {
            kind: "editable_focus",
            editing: isEditable(deepActiveElement()) || (await anyChildFrameEditing()),
          };
          break;
        case "frame_fields": {
          const all = await allFieldEntries();
          response = {
            kind: "frame_fields",
            fields: all.map((entry, index) => ({ index, rect: entry.rect, active: entry.active })),
          };
          break;
        }
        case "frame_focus_field": {
          const all = await allFieldEntries();
          const chosen = all[request.index || 0];
          response = chosen ? await focusFieldEntry(chosen) : { kind: "ok" };
          break;
        }
        case "frame_focus_last": {
          if (lastFieldFrame && lastFieldFrame.isConnected) {
            const sub = await evalInFrame(lastFieldFrame, { op: "frame_focus_last" });
            if (sub && sub.kind === "activated") {
              response = sub;
              break;
            }
          }
          const last = lastInput();
          response = last && isEditable(last) && isRendered(last) ? focusField(last) : { kind: "ok" };
          break;
        }
        case "scroll_if_able":
          response = await scrollIfAble(request);
          break;
        case "copy_selection":
          response = await copySelection();
          break;
        case "copy_selection_if_any":
          response = await copySelectionIfAny();
          break;
        case "cut_selection":
          response = await cutSelection();
          break;
        case "cut_selection_if_any":
          response = await cutSelectionIfAny();
          break;
        case "collect_controls":
          response = { kind: "raw_controls", items: await collectControlsRaw() };
          break;
        case "captcha_tokens":
          response = { kind: "captcha_tokens", tokens: await captchaTokens() };
          break;
        case "captcha_areas":
          response = {
            kind: "captcha_areas",
            areas: await captchaAreas(),
            captcha_tokens: window.top === window ? await captchaTokens() : 0,
            viewport: [innerWidth, innerHeight],
          };
          break;
        case "jump_scan": {
          if (request.watch_mutations) enableMutationWatch();
          const targets = await jumpTargets(request);
          const includeCaptchaMeta =
            request.jump === "captcha" || request.jump === "challenge" || request.jump === "hints";
          const includeAreas =
            request.jump === "captcha" || request.jump === "challenge" || targets.some(t => t.challenge);
          response = {
            kind: "targets",
            jump: request.jump,
            pattern: request.pattern || "",
            targets,
            captcha_tokens: includeCaptchaMeta ? await captchaTokens() : 0,
            captcha_areas: includeAreas ? await captchaAreas() : [],
            viewport: [innerWidth, innerHeight],
          };
          break;
        }
        case "jump_pick":
          response = await pick(request.id, !!request.new_tab, !!request.extend);
          break;
        case "captcha_fill":
          response = await captchaFill(request.text);
          break;
        case "dispatch_touch":
          response = await dispatchTouch(request.stage, request.x, request.y, !!request.reset_chain);
          break;
        case "refocus_top":
          response = refocusTop();
          break;
        case "scroll":
          response = await scroll(request);
          break;
        case "input_choose":
          response = await inputChoose(request.count);
          break;
        case "input_first":
          response = await inputFirst(request.count);
          break;
        case "input_last":
          response = await inputLast(request.count);
          break;
        case "input_step":
          response = await inputStep(request.delta || 1);
          break;
        case "blur_field":
          response = await blurField();
          break;
        case "edit_delete_to_line_start":
          response = await editDeleteToLineStart();
          break;
        case "edit_delete_to_line_end":
          response = await editDeleteToLineEnd();
          break;
        case "edit_delete_char_forward":
          response = await editDeleteCharForward();
          break;
        case "visual_enter":
          response = await visualEnter();
          break;
        case "visual_exit":
          response = await visualExit();
          break;
        case "visual_yank":
          response = await visualYank();
          break;
        case "visual_swap_ends":
          response = await visualSwapEnds();
          break;
        case "visual_move":
          response = await visualMove(request.motion, request.count);
          break;
        case "visual_text_object":
          response = await visualObject(request.keys || "iw", request.count);
          break;
        case "caret":
          response = await visualCaret();
          break;
        case "yank_put":
          response = await yankPut(request.text, request.index || 0, !!request.replace);
          break;
        case "yank_cycle":
          response = await yankCycle(request.step || 1);
          break;
        case "captcha_action":
          response = await captchaAction(request.action);
          break;
        case "toggle_reader_mode":
          response = toggleReaderMode();
          break;
        default:
          response = { kind: "message", text: "Unknown automation request: " + request.op };
      }
      return JSON.stringify(response);
    } catch (error) {
      return JSON.stringify({ kind: "message", text: String(error) });
    }
  };

  function toggleReaderMode() {
    const existing = document.getElementById("__zendia-reader-overlay");
    if (existing) {
      const prev = existing.getAttribute("data-prev-overflow") || "";
      existing.remove();
      if (document.documentElement) {
        document.documentElement.style.overflow = prev;
      }
      return { kind: "reader_mode", active: false };
    }
    const proto = String((window.location && window.location.protocol) || "");
    if (proto === "about:" || proto === "data:" && !document.body) {
      return { kind: "reader_mode", active: false, message: "Reader view is not available on this page" };
    }
    let root = document.querySelector("article, [role='main'], main");
    if (!root && document.body) {
      let best = null;
      let bestLen = 0;
      const candidates = document.body.querySelectorAll("section, div");
      for (let i = 0; i < candidates.length; i++) {
        const el = candidates[i];
        const pCount = el.querySelectorAll("p").length;
        if (pCount < 2) continue;
        const len = (el.innerText || el.textContent || "").trim().length;
        if (len > bestLen) {
          bestLen = len;
          best = el;
        }
      }
      root = best;
    }
    const textLen = root ? (root.innerText || root.textContent || "").trim().length : 0;
    if (!root || textLen < 140) {
      return { kind: "reader_mode", active: false, message: "Reader view is not available on this page" };
    }
    const overlay = document.createElement("div");
    overlay.id = "__zendia-reader-overlay";
    const prevOverflow = (document.documentElement && document.documentElement.style.overflow) || "";
    overlay.setAttribute("data-prev-overflow", prevOverflow);
    overlay.style.cssText = [
      "position:fixed",
      "inset:0",
      "z-index:2147483640",
      "overflow-y:auto",
      "background:#1a1b26",
      "color:#c0caf5",
      "padding:48px 24px 96px",
      "font:18px/1.7 Georgia, 'Times New Roman', serif",
      "box-sizing:border-box",
    ].join(";");
    const container = document.createElement("article");
    container.style.cssText = "max-width:680px;margin:0 auto;";
    const heading = document.createElement("h1");
    heading.textContent = document.title || "Reader View";
    heading.style.cssText = "font:700 30px/1.3 system-ui, sans-serif;margin:0 0 24px;color:#c0caf5;";
    container.appendChild(heading);
    const clone = root.cloneNode(true);
    const junk = clone.querySelectorAll("script, style, nav, aside, footer, iframe, form, button, svg");
    for (let i = 0; i < junk.length; i++) {
      junk[i].remove();
    }
    container.appendChild(clone);
    overlay.appendChild(container);
    (document.body || document.documentElement).appendChild(overlay);
    if (document.documentElement) {
      document.documentElement.style.overflow = "hidden";
    }
    overlay.focus();
    return { kind: "reader_mode", active: true };
  }
})();
