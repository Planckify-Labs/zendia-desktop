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
(function () {
  "use strict";

  if (typeof globalThis.__zendiaAutomation === "function") {
    return;
  }

  // ---------------------------------------------------------------------------------------------------------------
  // The DOM, across open shadow roots
  // ---------------------------------------------------------------------------------------------------------------

  const NOT_TEXT = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEXTAREA", "SELECT"]);
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
    while (element && element.shadowRoot && element.shadowRoot.activeElement) {
      element = element.shadowRoot.activeElement;
    }
    return element;
  }

  function* deepElements(root) {
    if (!root || !root.querySelectorAll) return;
    for (const element of root.querySelectorAll("*")) {
      yield element;
      if (element.shadowRoot) yield* deepElements(element.shadowRoot);
    }
  }

  // Text nodes in document order, those of a shadow root before its host's own children.
  function* deepTextNodes(root) {
    if (!root) return;
    const stack = [root];
    while (stack.length) {
      const node = stack.pop();
      if (node.nodeType === 3) {
        yield node;
        continue;
      }
      const children = [];
      if (node.shadowRoot) children.push(...node.shadowRoot.childNodes);
      children.push(...node.childNodes);
      for (let i = children.length - 1; i >= 0; i--) stack.push(children[i]);
    }
  }

  function deepElementFromPoint(x, y) {
    let element = document.elementFromPoint(x, y);
    while (element && element.shadowRoot && typeof element.shadowRoot.elementFromPoint === "function") {
      const inner = element.shadowRoot.elementFromPoint(x, y);
      if (!inner || inner === element) break;
      element = inner;
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

  // Whether an element is rendered and not hidden, as `checkVisibility()` with opacity and visibility checks says.
  function isRendered(element) {
    if (!element || !element.isConnected) return false;
    if (typeof element.checkVisibility === "function") {
      try {
        return element.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true });
      } catch (_) {}
    }
    for (let each = element; each && each.nodeType === 1; each = composedParent(each)) {
      const style = getComputedStyle(each);
      if (!style || style.display === "none" || style.contentVisibility === "hidden") return false;
      if (each === element && (style.visibility === "hidden" || style.visibility === "collapse")) return false;
      if (parseFloat(style.opacity) === 0) return false;
    }
    return element.getClientRects().length > 0;
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
  // Controls: what a user can press, type in or toggle
  // ---------------------------------------------------------------------------------------------------------------

  const CONTROLS = [
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
  const HANDS = new Set([
    "pointer",
    "grab",
    "grabbing",
    "move",
    "all-scroll",
    "ew-resize",
    "e-resize",
    "w-resize",
    "col-resize",
  ]);
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

  // The controls of the page; with `fields` "exclude" (`s`) or "only" (`i`). With a pattern only those whose text
  // contains it, each with the characters that follow the matches.
  function controlTargets(fields, pattern) {
    const from = center();
    const targets = [];
    const offered = new Map();
    for (const element of findControls()) {
      const field = isField(element);
      if (fields !== "include" && field !== (fields === "only")) continue;
      const text = pattern ? textOf(element) : "";
      const found = pattern ? findMatches(text, pattern) : [];
      if (pattern && !found.length) continue;
      const rect = element.getBoundingClientRect();
      const id = controlId(element);
      offered.set(id, { control: element });
      targets.push(
        target(id, rect, field ? "field" : "control", {
          nexts: found.map(match => match.next),
          distance: distance(rect, from),
          href: element.localName === "a" && element.href ? element.href : null,
        })
      );
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

  // The places a pattern occurs in the text on screen (`S`), nearest the cursor first once labeled.
  function textTargets(pattern) {
    const from = caretPoint() || center();
    const segments = [];
    for (const node of deepTextNodes(document.body || document.documentElement)) {
      if (segments.length >= MAX_SEGMENTS) break;
      const parent = composedParent(node);
      if (!node.data.trim() || !parent || NOT_TEXT.has(parent.tagName) || !isRendered(parent)) continue;
      const box = parent.getBoundingClientRect();
      if (box.bottom > 0 && box.top < innerHeight && box.right > 0 && box.left < innerWidth) segments.push(node);
    }
    const targets = [];
    const offered = new Map();
    outer: for (let s = 0; s < segments.length; s++) {
      const node = segments[s];
      for (const match of findMatches(node.data, pattern)) {
        const range = document.createRange();
        range.setStart(node, match.index);
        range.setEnd(node, match.index + pattern.length);
        const rect = range.getBoundingClientRect();
        if (rect.width && rect.bottom > 0 && rect.top < innerHeight && rect.right > 0 && rect.left < innerWidth) {
          const id = s + ":" + match.index;
          offered.set(id, { caret: { node, offset: match.index } });
          targets.push(target(id, rect, "match", { nexts: [match.next], distance: distance(rect, from) }));
        }
        if (targets.length >= MAX_MATCHES) break outer;
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

  // The part of an element that its scrolling and clipping ancestors leave to be seen, as the fork's visibleRect.
  function clippedRect(element, cache) {
    if (cache.has(element)) return cache.get(element);
    const rect = element.getBoundingClientRect();
    let left = rect.left;
    let top = rect.top;
    let right = rect.right;
    let bottom = rect.bottom;
    let level = 0;
    for (let each = composedParent(element); each && level < 12; each = composedParent(each), level++) {
      if (each.nodeType !== 1) continue;
      const style = getComputedStyle(each);
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

  const PAINTED_ELEMENTS = new Set(["img", "video", "canvas", "iframe", "svg", "embed", "object", "picture"]);

  // Whether an element draws something that hides what is under it: a picture, or a background that is mostly opaque.
  function paintsOver(element) {
    if (PAINTED_ELEMENTS.has(element.localName)) return true;
    const style = getComputedStyle(element);
    if (style.backgroundImage && style.backgroundImage !== "none") return true;
    const color = style.backgroundColor || "";
    if (!color || color === "transparent") return false;
    const parts = /\(([^)]*)\)/.exec(color);
    const values = parts ? parts[1].split(/[\s,/]+/).filter(Boolean) : [];
    if (values.length < 4) return true;
    const alpha = values[3].endsWith("%") ? parseFloat(values[3]) / 100 : parseFloat(values[3]);
    return alpha >= 0.5;
  }

  // Whether what the pointer hits over an element hides it: something between the hit and the element's nearest
  // shared ancestor draws over it. A link laid over a whole card to make it clickable draws nothing, so the text under
  // it still shows.
  function coveredBy(hit, element) {
    for (let each = hit; each && !composedContains(each, element); each = composedParent(each)) {
      if (each.nodeType === 1 && paintsOver(each)) return true;
    }
    return false;
  }

  // Whether a word of a text shows where it is: inside what clips its element, and not under something drawn over
  // it, such as a popup or a banner over the page.
  function wordShows(node, rect, clips, hits) {
    const element = composedParent(node);
    if (!element || element.nodeType !== 1) return false;
    if (rect.width < 2 || rect.height < 4) return false;
    const x = Math.min(Math.max(rect.left + rect.width / 2, 0), innerWidth - 1);
    const y = Math.min(Math.max(rect.top + rect.height / 2, 0), innerHeight - 1);
    const clip = clippedRect(element, clips);
    if (x < clip.left || x > clip.right || y < clip.top || y > clip.bottom) return false;
    const key = Math.round(y);
    let byLine = hits.get(element);
    if (!byLine) hits.set(element, (byLine = new Map()));
    if (!byLine.has(key)) {
      const hit = deepElementFromPoint(x, y);
      byLine.set(key, !hit || composedContains(element, hit) || !coveredBy(hit, element));
    }
    return byLine.get(key);
  }

  // The lines a paragraph's words that show are on, from its first: a block's text set apart elsewhere on the page
  // (a caption, a badge placed far away) is not part of its bar.
  function paragraphRect(rects) {
    if (!rects.length) return null;
    let { left, top, right, bottom } = rects[0];
    for (const rect of rects.slice(1)) {
      if (rect.top > bottom + Math.max(rect.height, 8)) break;
      left = Math.min(left, rect.left);
      top = Math.min(top, rect.top);
      right = Math.max(right, rect.right);
      bottom = Math.max(bottom, rect.bottom);
    }
    return { left, top, width: right - left, height: bottom - top, right, bottom };
  }

  // Every paragraph and word on screen that shows: a word label puts the cursor there (`land`) or grows the selection
  // to it, and a paragraph label selects the paragraph.
  function wordTargets() {
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
    const hits = new Map();
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
        const rect = range ? range.getBoundingClientRect() : null;
        if (!rect || !onScreen(rect) || !wordShows(range.startContainer, rect, clips, hits)) continue;
        shown.push(rect);
        const wid = "w" + (start + match.index);
        offered.set(wid, { caret: { node: range.startContainer, offset: range.startOffset } });
        words.push(target(wid, rect, "word", { distance: distance(rect, from) }));
      }
      // A paragraph none of whose words show is not there to pick.
      const rect = paragraphRect(shown);
      if (!rect) continue;
      const pid = "p" + start;
      offered.set(pid, { paragraph: lineRange });
      paragraphs.push(target(pid, rect, "paragraph", { distance: distance(rect, from) }));
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
    constructor() {
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
      let previousBlock = null;
      const parts = [];
      let length = 0;
      for (const node of deepTextNodes(document.body || document.documentElement)) {
        const parent = composedParent(node);
        if (!node.data || !parent || NOT_TEXT.has(parent.tagName)) continue;
        if (!visible.has(parent)) visible.set(parent, isRendered(parent));
        if (!visible.get(parent) || !node.data.trim()) continue;
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
  // it and is bigger.
  function chooseRange(ranges, selection) {
    const size = range => range.end - range.start;
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

  function findTextObject(text, kind, key, selection, times) {
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
    const from = starts[first];
    const to = starts[last + 1] === undefined ? text.length : starts[last + 1];
    const slice = text.slice(from, to);
    let current = { start: selection.start - from, end: selection.end - from };
    let result = null;
    for (let i = 0; i < times; i++) {
      const range = chooseRange(candidates(slice, key, kind), current);
      if (!range) break;
      result = range;
      current = range;
    }
    return result && { start: result.start + from, end: result.end + from };
  }

  // Where a paragraph motion goes: the end of the paragraph it is in (or the start going back), and from there the
  // next one's.
  function paragraphEdge(text, offset, forward) {
    const start = text.lastIndexOf("\n", offset - 1) + 1;
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

  function scroll(request) {
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

  // ---------------------------------------------------------------------------------------------------------------
  // Visual mode
  // ---------------------------------------------------------------------------------------------------------------

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

  // Scroll the selection's moving end into view, as little as it takes.
  function reveal() {
    const rect = focusRect();
    if (!rect) return;
    const margin = 8;
    let dy = 0;
    if (rect.top < 0) dy = rect.top - margin;
    else if (rect.bottom > innerHeight) dy = rect.bottom - innerHeight + margin;
    let dx = 0;
    if (rect.left < 0) dx = rect.left - margin;
    else if (rect.right > innerWidth) dx = rect.right - innerWidth + margin;
    if (dy) {
      const target = scrollTarget(true);
      if (target) target.scrollBy({ top: dy, behavior: "instant" });
    }
    if (dx) {
      const target = scrollTarget(false);
      if (target) target.scrollBy({ left: dx, behavior: "instant" });
    }
  }

  function visualState(active, extra) {
    return Object.assign({ kind: "visual_state", active, yanked: null, caret: active ? caret() : null }, extra);
  }

  function firstVisibleText() {
    for (const node of deepTextNodes(document.body || document.documentElement)) {
      const parent = composedParent(node);
      if (!node.data.trim() || !parent || NOT_TEXT.has(parent.tagName) || !isRendered(parent)) continue;
      const offset = node.data.search(/\S/);
      const range = document.createRange();
      range.setStart(node, offset);
      range.setEnd(node, offset + 1);
      const rect = range.getBoundingClientRect();
      if (rect.width && rect.bottom > 0 && rect.top < innerHeight) return { node, offset };
    }
    return null;
  }

  function focusOnScreen() {
    const rect = focusRect();
    return !!rect && rect.bottom > 0 && rect.top < innerHeight && rect.right > 0 && rect.left < innerWidth;
  }

  // `v`: the cursor stays where the selection is when it is on screen, else it starts at the first text on screen.
  // A collapsed cursor asks for labels on every word and paragraph, to say where to land.
  function visualEnter() {
    const selection = getSelection();
    if (!selection) return visualState(false);
    if (!selection.rangeCount || !focusOnScreen()) {
      const start = firstVisibleText();
      if (!start) return visualState(false);
      selection.collapse(start.node, start.offset);
    }
    return visualState(true, { land: selection.isCollapsed });
  }

  // The cursor stays where the selection ended.
  function visualExit() {
    const selection = getSelection();
    if (selection && selection.focusNode) selection.collapse(selection.focusNode, selection.focusOffset);
    return visualState(false);
  }

  function visualSwapEnds() {
    const selection = getSelection();
    if (selection && selection.anchorNode && selection.focusNode) {
      const { anchorNode, anchorOffset, focusNode, focusOffset } = selection;
      selection.setBaseAndExtent(focusNode, focusOffset, anchorNode, anchorOffset);
      reveal();
    }
    return visualState(true);
  }

  const SELECTION_MOTIONS = {
    left: ["left", "character"],
    right: ["right", "character"],
    down: ["forward", "line"],
    up: ["backward", "line"],
    word_forward: ["forward", "word"],
    word_backward: ["backward", "word"],
    line_start: ["backward", "lineboundary"],
    line_end: ["forward", "lineboundary"],
  };

  function visualMove(motion, count) {
    const selection = getSelection();
    if (!selection || !selection.focusNode) return visualState(true);
    const times = Math.max(1, count || 1);
    const spec = SELECTION_MOTIONS[motion];
    if (spec) {
      // The line boundaries take no count.
      const steps = motion === "line_start" || motion === "line_end" ? 1 : times;
      for (let i = 0; i < steps; i++) selection.modify("extend", spec[0], spec[1]);
      reveal();
      return visualState(true);
    }
    // Over the page's text: the paragraphs, and the top and the bottom, which also scroll there as `gg` and `G` do.
    const page = new PageText();
    let place = { offset: page.offsetOf(selection.focusNode, selection.focusOffset) };
    const find = {
      paragraph_forward: (text, offset) => paragraphEdge(text, offset, true),
      paragraph_backward: (text, offset) => paragraphEdge(text, offset, false),
      top: () => ({ offset: 0, side: "start" }),
      bottom: text => ({ offset: text.length, side: "end" }),
    }[motion];
    if (!find) return visualState(true);
    const steps = motion === "top" || motion === "bottom" ? 1 : times;
    for (let i = 0; i < steps; i++) place = find(page.text, place.offset);
    const point = page.pointAt(place.offset, place.side);
    if (point) {
      selection.extend(point.node, point.offset);
      if (motion === "top" || motion === "bottom") scroll({ motion, count: 1 });
      else reveal();
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

  function visualObject(keys, count) {
    const selection = getSelection();
    if (!selection || !selection.rangeCount) return visualState(true);
    const kind = keys[0];
    const key = { "<lt>": "<", "<Bar>": "|", "<Bslash>": "\\" }[keys.slice(1)] || keys.slice(1);
    const times = Math.max(1, count || 1);
    const page = new PageText();
    let current = selectedRange(page, selection);
    let range = null;
    if (key === "t") {
      const ranges = elementRanges(page, selection);
      for (let i = 0; i < times; i++) {
        const grown = chooseRange(ranges, current);
        if (!grown) {
          range = null;
          break;
        }
        range = grown;
        current = grown;
      }
    } else {
      range = findTextObject(page.text, kind, key, current, times);
    }
    if (range) {
      const start = page.pointAt(range.start, "start");
      const end = page.pointAt(range.end, "end");
      if (start && end) {
        selection.setBaseAndExtent(start.node, start.offset, end.node, end.offset);
        reveal();
      }
    }
    return visualState(true);
  }

  // `y`: the selection's text, the rects to flash, and the cursor at where the yank began.
  function visualYank() {
    const selection = getSelection();
    let yanked = null;
    let flash = [];
    if (selection && selection.rangeCount && !selection.isCollapsed) {
      yanked = selection.toString();
      const ranges = [];
      for (let i = 0; i < selection.rangeCount; i++) ranges.push(selection.getRangeAt(i).cloneRange());
      for (const range of ranges) for (const rect of range.getClientRects()) flash.push(plainRect(rect));
      selection.collapse(ranges[0].startContainer, ranges[0].startOffset);
    }
    return { kind: "visual_state", active: false, yanked, caret: null, flash };
  }

  // A label pick on text: the selection grows to it in visual mode; otherwise visual mode starts there.
  function moveCaret(node, offset, extend) {
    const selection = getSelection();
    if (extend && selection.rangeCount) selection.extend(node, offset);
    else selection.collapse(node, offset);
    reveal();
    return visualState(true);
  }

  // ---------------------------------------------------------------------------------------------------------------
  // Fields
  // ---------------------------------------------------------------------------------------------------------------

  const ROW_TOLERANCE = 4;
  let lastField = null;
  let lastPut = null;

  document.addEventListener(
    "focusin",
    event => {
      const element = event.composedPath ? event.composedPath()[0] : event.target;
      if (isEditable(element)) lastField = typeof WeakRef === "function" ? new WeakRef(element) : element;
    },
    true
  );

  function lastInput() {
    const element = lastField && lastField.deref ? lastField.deref() : lastField;
    return element && element.isConnected ? element : null;
  }

  function isShownField(element) {
    if (!isField(element) || !isRendered(element)) return false;
    const rect = element.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  }

  // The page's fields in reading order: top to bottom, then left to right.
  function fields() {
    return [...deepElements(document)]
      .filter(isShownField)
      .map(element => ({ element, rect: element.getBoundingClientRect() }))
      .sort((a, b) => (Math.abs(a.rect.top - b.rect.top) > ROW_TOLERANCE ? a.rect.top - b.rect.top : a.rect.left - b.rect.left))
      .map(item => item.element);
  }

  function inViewport(element) {
    const rect = element.getBoundingClientRect();
    return rect.bottom > 0 && rect.right > 0 && rect.top < innerHeight && rect.left < innerWidth;
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

  function inputFirst(count) {
    const visible = fields().filter(inViewport);
    // No field to type in: the keys go to the page, as with `I`.
    if (!visible.length) return { kind: "explicit_insert" };
    return focusField(visible[Math.min(Math.max(count || 1, 1), visible.length) - 1]);
  }

  // `i`: a count says which field and one field needs no choosing; several are labeled.
  function inputChoose(count) {
    if (count === null || count === undefined) {
      const choices = controlTargets("only", "");
      if (choices.length > 1) return { kind: "targets", jump: "inputs", pattern: "", targets: choices };
      if (choices.length === 1) return pick(choices[0].id, false, false);
    }
    return inputFirst(count);
  }

  function inputLast(count) {
    const last = lastInput();
    if (last && isEditable(last) && isRendered(last)) return focusField(last);
    const list = fields();
    if (list.length) return focusField(list[Math.min(Math.max(count || 1, 1), list.length) - 1]);
    return { kind: "ok" };
  }

  function inputStep(offset) {
    const list = fields();
    if (!list.length) return { kind: "ok" };
    const current = list.indexOf(deepActiveElement());
    let index;
    if (current === -1) {
      // Nothing focused: start from what is on screen.
      const shown = list.findIndex(inViewport);
      const start = shown === -1 ? 0 : shown;
      index = offset > 0 ? start + offset - 1 : start + offset;
    } else {
      index = current + offset;
    }
    return focusField(list[((index % list.length) + list.length) % list.length]);
  }

  // `<Esc>` in insert mode: the field gives up the focus, so typing no longer reaches it.
  function blurField() {
    const active = deepActiveElement();
    if (isEditable(active)) active.blur();
    return { kind: "ok" };
  }

  function isTextField(element) {
    return !!element && (element.localName === "input" || element.localName === "textarea");
  }

  // Put a yank where the caret is, or over the put the caret still sits at the end of. The editor's own insert keeps
  // undo and fires input events, so the page sees typing.
  function yankPut(text, index, replace) {
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
  function yankCycle(step) {
    const field = deepActiveElement();
    if (!isEditable(field) || field.localName === "select") return { kind: "ok" };
    const last = cyclingPut();
    return { kind: "cycle", index: last ? Math.max(last.index + step, 0) : 0 };
  }

  // ---------------------------------------------------------------------------------------------------------------
  // Picking a target
  // ---------------------------------------------------------------------------------------------------------------

  // A field takes the focus in insert mode; any other control is pressed by the browser where it is on top, as a
  // user's click would be; text moves the cursor.
  function pick(id, newTab, extend) {
    const entry = pickable.get(id);
    if (!entry) return { kind: "ok" };
    if (entry.control) {
      const element = entry.control;
      if (!element.isConnected) return { kind: "ok" };
      if (isEditable(element)) return focusField(element);
      if (newTab && element.localName === "a" && element.href) {
        return { kind: "activated", mode: "n", url: element.href, message: null };
      }
      const point = pointOf(element);
      return { kind: "press", x: point.x, y: point.y };
    }
    if (entry.paragraph) {
      const range = entry.paragraph;
      getSelection().setBaseAndExtent(range.startContainer, range.startOffset, range.endContainer, range.endOffset);
      reveal();
      return visualState(true);
    }
    if (entry.caret) return moveCaret(entry.caret.node, entry.caret.offset, extend);
    return { kind: "ok" };
  }

  // ---------------------------------------------------------------------------------------------------------------
  // CAPTCHA helpers (the keys' simple form; the fork's full CAPTCHA jump is not ported)
  // ---------------------------------------------------------------------------------------------------------------

  function isCaptchaControl(element) {
    const name = ((typeof element.className === "string" ? element.className : "") + " " + (element.id || "")).toLowerCase();
    const label = (element.getAttribute("aria-label") || "").toLowerCase();
    const src = (element.getAttribute("src") || "").toLowerCase();
    return /captcha|rc-image|rc-anchor|challenge|turnstile/.test(name + " " + label + " " + src);
  }

  function captchaTargets() {
    const from = center();
    const targets = [];
    const offered = new Map();
    for (const element of findControls()) {
      let inCaptcha = false;
      for (let each = element; each && !inCaptcha; each = composedParent(each)) inCaptcha = each.nodeType === 1 && isCaptchaControl(each);
      if (!inCaptcha) continue;
      const rect = element.getBoundingClientRect();
      const id = controlId(element);
      offered.set(id, { control: element });
      targets.push(target(id, rect, isField(element) ? "field" : "control", { distance: distance(rect, from) }));
    }
    pickable = offered;
    return targets;
  }

  function captchaAction(action) {
    const patterns = {
      verify: /verify|next|skip|submit|continue|confirm/i,
      reload: /reload|refresh|new challenge|another/i,
      audio: /audio|image challenge|visual challenge/i,
      play: /play|listen/i,
    };
    const controls = captchaTargets().map(each => pickable.get(each.id).control);
    if (action === "answer") {
      const field = controls.find(isField);
      return field ? focusField(field) : { kind: "ok" };
    }
    const pattern = patterns[action];
    const control = pattern && controls.find(each => !isField(each) && pattern.test(textOf(each) + " " + (each.id || "")));
    if (!control) return { kind: "ok" };
    const point = pointOf(control);
    return { kind: "press", x: point.x, y: point.y };
  }

  // ---------------------------------------------------------------------------------------------------------------
  // Requests
  // ---------------------------------------------------------------------------------------------------------------

  function jumpTargets(request) {
    const pattern = request.pattern || "";
    switch (request.jump) {
      case "hints":
        return controlTargets("exclude", "");
      case "controls":
        return controlTargets("include", pattern);
      case "inputs":
        return controlTargets("only", "");
      case "text":
        return pattern ? textTargets(pattern) : [];
      case "words":
      case "land":
        return wordTargets();
      case "captcha":
        return captchaTargets();
    }
    return [];
  }

  globalThis.__zendiaAutomation = function (requestJson) {
    try {
      const request = JSON.parse(requestJson);
      let response;
      switch (request.op) {
        case "jump_scan":
          response = {
            kind: "targets",
            jump: request.jump,
            pattern: request.pattern || "",
            targets: jumpTargets(request),
          };
          break;
        case "jump_pick":
          response = pick(request.id, !!request.new_tab, !!request.extend);
          break;
        case "scroll":
          response = scroll(request);
          break;
        case "input_choose":
          response = inputChoose(request.count);
          break;
        case "input_first":
          response = inputFirst(request.count);
          break;
        case "input_last":
          response = inputLast(request.count);
          break;
        case "input_step":
          response = inputStep(request.delta || 1);
          break;
        case "blur_field":
          response = blurField();
          break;
        case "visual_enter":
          response = visualEnter();
          break;
        case "visual_exit":
          response = visualExit();
          break;
        case "visual_yank":
          response = visualYank();
          break;
        case "visual_swap_ends":
          response = visualSwapEnds();
          break;
        case "visual_move":
          response = visualMove(request.motion, request.count);
          break;
        case "visual_text_object":
          response = visualObject(request.keys || "iw", request.count);
          break;
        case "caret":
          response = visualState(true);
          break;
        case "yank_put":
          response = yankPut(request.text, request.index || 0, !!request.replace);
          break;
        case "yank_cycle":
          response = yankCycle(request.step || 1);
          break;
        case "captcha_action":
          response = captchaAction(request.action);
          break;
        default:
          response = { kind: "message", text: "Unknown automation request: " + request.op };
      }
      return JSON.stringify(response);
    } catch (error) {
      return JSON.stringify({ kind: "message", text: String(error) });
    }
  };
})();
