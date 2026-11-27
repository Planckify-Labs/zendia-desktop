# Vim mode parity with the old Zendia

Vim mode has been ported to the Freya window, but it does not yet behave like the old fork. This document lists every
difference found, states what the old fork does in enough detail to rebuild it, and says where the port needs work. It
is the checklist for finishing the port.

**The bar:** vim mode in the port is identical to the old fork in behaviour, look and text. A difference is a defect
unless it is listed under [Approved differences](#approved-differences).

Audited on 2026-11-27: the port at `66515309baf` against the old fork at `~/zendia` `7c8241f` (release 0.2.0b, the
build installed in `~/.local/opt/zendia`).

## How to use this document

1. Fix the [P0 defects](#fix-these-first) first.
2. Read [Root causes](#root-causes) before fixing anything else. Most defects share a few causes, and a patch that
   works around a cause will have to be redone.
3. Work through one area at a time. Each area has the fork's behaviour (the target), then a table of defects.
4. Check [Appendix A](#appendix-a-every-key-mapping) when an area is done: every mapping of the fork is listed there,
   so none can be forgotten.
5. An area is done when its acceptance checks pass, including the matching tests of the old fork
   ([Appendix B](#appendix-b-the-old-forks-tests-as-acceptance-checks)).

Severity:

| Level | Meaning |
| --- | --- |
| P0 | Loses the user's work, or makes a basic activity impossible (typing, standard shortcuts, acting on the right tab) |
| P1 | A feature of the fork is missing or does not work |
| P2 | Works, but behaves differently |
| P3 | Looks or reads differently |

Evidence:

| Mark | Meaning |
| --- | --- |
| L | Reproduced in the real window on a virtual display, 2026-11-27 |
| T | Reproduced by a test that drove the browser core with the vim profile's mappings |
| C | Read from the source of both sides; not run |
| R | Suspected from the source; check it in the running browser first |

Paths starting with `Zendia/` are in this repository. Paths starting with `fork:` are under
`~/zendia/src/zen/zendia/` unless they give a longer path. The old fork is our own project, so its code is free to
reuse here (confirmed 2026-11-27).

## Fix these first

| ID | What happens now | Evidence |
| --- | --- | --- |
| [INS-04](#insert-mode-and-fields) | **After `i`, typed characters often never reach the page**: at startup, after switching tabs or panes, and more (reported from use) | L, C |
| [EXP-01](#explorer-sidebar) | In the sidebar, `d` closes the **active** tab, not the selected row. `c p e M U R r >` also act on the active tab | L, T |
| [INS-01](#insert-mode-and-fields) | Email, number and date fields and `<select>` never enter insert mode. Typing runs vim keys: `r` reloads the page, `p` loads the clipboard's address | L, C |
| [RT-02](#key-routing) | Mapped letters cannot be typed into the web chat panel or an extension popup while vim mode is on (also [EXT-02](#extensions)) | C |
| [RT-01](#key-routing) | Vim mode turns off every standard shortcut: Ctrl+T, Ctrl+W, Ctrl+Tab, F5, Alt+Left, zoom and the rest | L, T, C |
| [JMP-01](#jump-labels-s-gs-s-and-i) | On a zoomed page, picking a label clicks the wrong place | C |
| [NAV-20](#tabs-history-addresses-find-spaces-and-session) | After renaming a space from the keys with the library closed, jump labels stop taking letters | L, C |
| [WIN-01](#splits-panels-and-window-mode) | `<leader>\|` and `<leader>-` cannot split a tab (reported from use) | L, T |

## Root causes

| # | Cause | Where | Defects it produces |
| --- | --- | --- | --- |
| A | One input mode, one key engine and one jump for the whole window. The fork kept them **per page**, so each tab and each panel page had its own | `Zendia/Libraries/BrowserCore/src/stub/vim.rs`, `UI/Freya/src/ui/page/keymap_context.rs` | MD-01 to MD-03, RT-02, SL-03 |
| B | The Automation World script runs only in the top document of a page | `Services/WebContent/PageClient.cpp:2950`, `Zendia/Libraries/Automation` | FRM-01, CAP-01 |
| C | "A field has the focus" is read from the input method state, which is only on for inputs that have a text selection | `Services/WebContent/ConnectionFromClient.cpp:3429`, `UI/Freya/src/hub_adapter.rs:499` | INS-01, RT-02 |
| D | With vim on, keys go to the vim engine **instead of** the standard keymap. The fork added vim on top of the browser's shortcuts | `UI/Freya/src/ui/page/keymap_context.rs:179` | RT-01 |
| E | Commands run from a surface carry no target, so they act on the active tab or space | `stub.rs:654` (`resolve_target_tab`), `stub/chrome_actions.rs:52` | EXP-01, EXP-10, LIB-01 |
| F | The library and the menus read keys in their own handlers, outside the engine | `UI/Freya/src/ui/library/nav.rs:382`, `ui/kit/list_nav.rs:100` | LIB-02, LIB-06, MN-01 |
| G | The split model is a flat list (horizontal, vertical or grid). The fork had a layout tree | `Zendia/Libraries/BrowserCore/src/split.rs` | WIN-08 to WIN-10 |

## Approved differences

Do not "fix" these without asking.

| Difference | Where it came from |
| --- | --- |
| A field that a page focuses by itself puts the page in insert mode. The fork refused such a focus (up to 3 times, until the user did something) | Requested 2026-11-27 |
| Boosts are not ported, so `<leader>ob` and `gb` have no mapping | `FreyaUIPort.md`, "Not ported"; confirmed 2026-11-27 |

These were decided **before** the identical bar was restated. Confirm that each still stands:

| Difference | Where it came from |
| --- | --- |
| `v` leaves out words that are clipped or covered, and a paragraph's bar spans its connected lines only | Fix made 2026-11-27 after `v` was reported broken |
| `<leader>ff` opens the URL bar on the page's address. The fork opened it empty, as a search over tabs, history and bookmarks | Commit `bf45459f719` |
| Vim mode is off until the user turns it on, also as the choice offered in the first-run setup. The fork had it on by default | `Architecture.md`, "Vim mode (optional)" |

An earlier approval for larger jump labels was withdrawn on 2026-11-27 after use ("the labeling is not really as good as
the old forks"): [JMP-06](#jump-labels-s-gs-s-and-i).

`FreyaUIPort.md` also lists session restore and "CAPTCHA watching and solving" as outside the UI port. The CAPTCHA
**keys** are in scope and important (stated 2026-11-27); the watcher and the solving agent are not covered here.

## Key engine and configuration

The engine itself matches the fork (`fork: core/ZendiaKeyEngine.sys.mjs` against
`Zendia/Libraries/Keymap/src/engine.rs`): modes, counts, multi-key sequences, `<Esc>` and `<BS>` on pending keys,
no timeout on a pure prefix, 300 ms `timeoutlen` on an ambiguous one, and the fork's engine tests were ported case for
case. What differs is everything that fed it.

Fork behaviour (`fork: Zendia.sys.mjs`, `config/ZendiaOptions.sys.mjs`):

- Preferences, applied live: `zendia.enabled`, `zendia.leader` (default `<Space>`), `zendia.timeoutlen` (300),
  `zendia.statusline.enabled`, `zendia.statusline.show` (`shrink`, `always`, `change`).
- A user file, `<profile>/zendia/zendia.json`: `extras`, plugin option overrides and `keymaps`. User maps have priority
  1000; a value of `false` removes a map.
- The explorer, toolbar, library and window modes inherit every `<leader>` map and group of normal mode that they do
  not define themselves. (Ported.)
- Turning vim mode on changes layout defaults: compact mode at startup, sidebar and toolbar hidden, and the sidebar
  stays hidden on the empty tab until the first key or click. The user's own choice still wins.
- The app menu has a "Vim mode" switch above Settings that leaves the menu open. Its tooltips are "Neovim-style keys
  drive the browser. Click to turn them off." and "Click to drive the browser with Neovim-style keys."

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| CFG-01 | P1 | C | Turning vim on or off in Settings or the app menu is not saved. A restart brings back the onboarding choice (`stub.rs:2999`) | Persist the setting where it is changed |
| CFG-02 | P2 | C | The statusline resting style is not saved | Persist it |
| CFG-03 | P1 | T | Turning vim on applies none of the layout defaults | Apply and restore them as the fork does |
| CFG-04 | P1 | C | No user keymap file: the profile is compiled in (`keymap_context.rs:53`) | Load user profiles over the base profile |
| CFG-05 | P1 | C | No leader or `timeoutlen` setting | Add both, applied live |
| CFG-06 | P3 | C | Notation: the fork reads an unknown or unclosed `<` as a literal and a space as `<Space>`. The port rejects the first and ignores the second | Match once user maps exist |
| CFG-07 | P3 | C | The Vim settings page says standard shortcuts remain available, which is false until RT-01 is fixed | Fix RT-01, then check the text |

## Key routing

Fork behaviour (`fork: actors/ZendiaKeysChild.sys.mjs`, `Zendia.sys.mjs`, `core/ZendiaPageKeys.sys.mjs`):

- A key the keymap does not use goes on to the page and then to the browser's own shortcuts. In normal mode Ctrl+W
  closes the tab; in a field it deletes a word.
- A key that was taken is taken completely: its keyup never reaches the page (reCAPTCHA's checkbox acts on Space keyup).
- A key held by IME composition, a bare modifier or a dead key is never a vim key. AltGr types its character. With
  Ctrl, Alt or Super, the physical key counts. (Ported.)
- Every page has its own engine: tabs, the web chat's page and extension popups. A field in any of them is insert mode.
- Keys typed while the focus is on the sidebar or other browser UI go to the page. Space presses a control that has the
  keyboard focus ring.
- Keys typed in the browser's own fields are theirs; insert mode's editing keys work there while vim is on.
- While a page is idle in normal mode, its browser-side keys run at once, even while it loads. Page-side sequences are
  replayed to the page in the order typed, so nothing typed ahead is lost or reordered.
- `<Esc>` in insert mode, or an unused `<Esc>` in normal mode, closes the web chat panel showing that page.
- Nothing in the window reacts to vim keys during onboarding.

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| RT-01 | P0 | L, T, C | With vim on, only the vim tables are consulted. Unused keys never reach `standard.toml`: Ctrl+T, Ctrl+W (normal), Ctrl+Tab, Ctrl+Shift+T, Ctrl+R, F5, Alt+Left, Ctrl+=, Ctrl+-, Ctrl+Q, Ctrl+N and others do nothing | When the engine does not use a key and has nothing pending, resolve it in the standard table. Vim maps win where both have the key |
| RT-02 | P0 | C | A web chat or extension popup page uses the active tab's mode (`surface.rs:313`). A mapped letter is taken from the page, then dropped (`browser.rs:966`, `:987`) or run on the tab. Their fields never report focus, as `hub_adapter.rs:485` looks them up as tabs | Give each page view its own mode and field state |
| RT-03 | P1 | C | While labels, the explorer, the toolbar or the library keep keys, keydown is withheld but keyup is forwarded (`surface.rs:310` against `:334`) | Record every withheld key so its keyup is withheld too |
| RT-04 | P1 | C | Page actions answer asynchronously and the keys typed meanwhile run as vim keys. `i` with several fields captures nothing until its labels arrive (`stub/page_actions.rs`, `InputChoose`); text typed right after `i` or `gi` runs commands until insert mode is reported | Hold keys while a page action that changes mode is in flight, then handle them in order (`start_jump` already does this for jumps) |
| RT-05 | P2 | C | The browser's own text fields apply Ctrl+W, Ctrl+U and Ctrl+H with vim **off** too (`text_field.rs:208`), so Ctrl+W in the URL bar no longer closes the tab. Other insert maps do not apply there | Apply insert maps there only with vim on |
| RT-06 | P2 | R | The which-key popup and the window both handle Backspace, which may go back two levels | Let the engine alone handle it |

## Modes

| Mode | Entered by | Left by | Unused keys | Counts |
| --- | --- | --- | --- | --- |
| Normal | default | | go to page and browser | yes |
| Insert | a field taking focus, `I`, `i`, `gi`, a label on a field | `<Esc>`; focus leaving the field (not after `I`) | go to page | no |
| Visual | `v`, `S`, a paragraph or word label | `<Esc>`, `v`, `y`, clicking into a field | swallowed | yes |
| Explorer | `<leader>e` | `q`, `<leader>e`, `<Esc>` with no selection, opening a tab, clicking outside | left alone | yes |
| Toolbar | `<leader>m` | `q`, `<Esc>`, `<leader>m`, pressing an item, clicking outside | left alone | yes |
| Library | opening the library | `q`, `<Esc>` in the list; clicking the page hands keys to it | left alone | yes |
| Window | `<leader>w<Space>` | `q`, `<Esc>` with nothing carried, any unused key (which then does its usual job), a click outside | ends the mode | yes |

Only one of explorer, toolbar, library and window mode is open at a time. A key that moved to another tab leaves that
page in normal mode even if it kept a field focused (ported). A **clicked** tab keeps its field and is in insert mode
(ported).

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| MD-01 | P1 | T | There is one mode for the window. Switching tabs and back loses visual mode and `I` insert mode (`stub/vim.rs:49`). Pending keys are shared too | Keep mode, explicit insert and pending keys per page |
| MD-02 | P1 | T | Visual mode survives a navigation | A new document starts in normal mode |
| MD-03 | P1 | T | Jump labels survive a tab switch and a navigation, and keep taking keys, so typing on the new tab picks hidden labels | End the jump on tab switch, navigation and page unload; keep it per page |

## Insert mode and fields

Fork behaviour (`fork: plugins/ZendiaInputs*.sys.mjs`, `ZendiaEdit*.sys.mjs`, `core/ZendiaDom.sys.mjs`):

- Editable means: contenteditable, `designMode`, an enabled `textarea` or `select`, or an input that is not
  `button checkbox color file hidden image radio range reset submit` and not read only or disabled. So **email, number,
  date and `select` all count**.
- `I`: insert mode that stays until `<Esc>`, whatever has the focus.
- `i`: [labels when there are several fields](#i-choosing-a-field). With a count, the count-th visible field.
- `gi`: the field last typed in on this page; otherwise the count-th field.
- `]i` `[i`, and `<C-j>` `<C-k>` in insert mode: next and previous field, wrapping, counted. Fields are ordered top to
  bottom (rows within 4 px), then left to right. With nothing focused, start from the first field on screen.
- Every focus by these keys puts the caret at the end of the field's text.
- `<Esc>`: leave the field, normal mode.
- `<C-w>` `<C-u>` `<C-h>`: delete a word back, to the line start, one character, using the editor's own commands, so
  undo and input events work.

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| INS-04 | P0 | L, C | **Characters typed after vim moves to a page do not reach it** until the page is clicked (table below). The field is chosen, but the page does not have the window's keyboard focus: a page surface takes it on a mouse press (`ui/page/surface.rs:212`), and `request_focus_active_surface` returns it to the surface used last, which after a switch is gone | Whenever the page that should get keys changes, and at startup and after anything closes, move keyboard focus to that page's surface |
| INS-01 | P0 | L, C | A field counts as focused only when the input method is on. That is false for every input without a text selection (`email`, `number`, `date`, `time`, `month`, `week`, `datetime-local`; `HTMLInputElement.cpp:4110`) and for `select`. The page stays in normal mode while the user types | Report "an editable element has the focus" on its own (extend the embedder seam, or report focus changes from the Automation World), with the fork's definition of editable |
| INS-02 | P2 | C | `gi` only remembers fields focused after the Automation World script was first loaded into the document (`automation_world.js:1262`) | Load the script with the document, or have the engine report the last field |
| INS-03 | P2 | R | `<C-u>` is "select to line start, then delete backward" (`hub_adapter.rs:637`). With the caret already at the line start this may delete the line break | Check, and delete only to the line start |

Where typing after `i` or `gi` worked in the real window (INS-04):

| Typing is lost | Typing works |
| --- | --- |
| right after startup, nothing clicked | after clicking the page |
| after `J` / `K` to another tab | after the sidebar is closed with `<Esc>` |
| after opening another tab with `<CR>` in the sidebar | after the URL bar is closed |
| in a new pane (`<leader>wa`), and after `<C-h>` to another pane | after window mode, toolbar mode and which-key close |
| after `<leader>wt`, and after reopening a tab with `u` | after a link or `H` `L` navigates the same tab |
| after cancelling a picker (`<leader>bj`, `<Esc>`) | |
| after the library is closed | |

The test for a fix: do each step on the left, press `i`, type, and see the text in the field.

`i`, `I`, `gi`, `]i`, `[i`, caret placement and ordering otherwise match. They do not reach fields inside frames
(FRM-01).

## Frames

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| FRM-01 | P1 | L, C | The script runs only in the top document (`PageClient.cpp:2950`). Nothing inside an `iframe`, same origin or not, gets labels, is found by `i` `]i` `gi`, can be reached by `S` or `v`, or is part of a CAPTCHA | Run the Automation World in every frame and aggregate in the browser |

Fork behaviour to rebuild (`fork: plugins/ZendiaJumpContent.sys.mjs:196-370`, `actors/ZendiaKeys*.sys.mjs`):

- The top page asks each visible frame (and they ask theirs) for its controls, and places their rects in its own
  viewport, scaled as the frame is drawn. A frame target is kept only if it shows inside the frame and nothing in the
  parent covers it. A frame that does not answer within 1 s is left out.
- Frames hidden with `visibility`, or with `pointer-events: none`, are skipped.
- A key typed in a frame runs page actions in the **top** page, which takes the focus. Field actions (`edit.*`,
  `input.*` except `i`, put, `I`) stay in the frame that has the field.
- While the page has labels up, keys typed in any of its frames go to the labels, except in a field.
- Treat everything a frame sends as untrusted.

## Scrolling

`j k h l` 60 px times the count; `<C-d>` `<C-u>` half the viewport; `<C-f>` `<C-b>` 0.9 of it; `gg` `G` top and bottom;
smooth. The target is the first scrollable element from the focused one upwards, else the document, else the largest
scroller. The port matches this (`automation_world.js:961-1032`). It differs only in what follows a scroll: see JMP-04
and SL-02.

## Tabs, history, addresses, find, spaces and session

Fork behaviour (`fork: plugins/ZendiaTabs.sys.mjs`, `ZendiaNavigation`, `ZendiaFind`, `ZendiaSession`, `ZendiaUi`):

- `J` `K`, `]b` `[b`: next and previous row of the sidebar, counted, wrapping. A split is one row and selects its last
  used pane. From the empty tab, next is the first and previous the last.
- `<leader>bb` and `` <leader>` ``: the most recently used other row. `]B` `[B` move the tab, counted.
- `<leader>bf`: a picker titled "Move to Folder" with "New Folder" (key `n`) and the space's other folders.
- `<leader>bj`: the sidebar is shown, its rows are labeled, a label selects that tab.
- `<leader>bm` opens the current row's context menu on that row; `<leader>bR` renames in the sidebar. Space rename,
  icon and theme also show the sidebar and wait for their popup.
- `<C-o>` `<C-i>`, `H` `L`: back and forward; a count jumps that many entries at once.
- `-`: parent path (http and https only; query and fragment dropped).
- `p` `P`: open the clipboard's address here or in a new tab. The text is trimmed, must be one line, is fixed up like
  typed text (a search for anything that is not an address), and **only http and https results are opened**.
- `/` opens the find bar; `n` `N` repeat. The picker keys open the URL bar restricted to tabs, history or search.
- `<leader>qq` quit, `qm` minimize, `qM` maximize, `qw` close **this window** (asking first when needed), `qs` restore
  the last session.
- `<leader><Tab><Tab>` opens the new space form, `<leader><Tab>d` asks before deleting the space and
  `<leader><Tab>u` unloads its tabs.
- `<leader>x`: the extensions picker. Everything about extensions is in [Extensions](#extensions).
- `<leader>E` expands or collapses the sidebar and nothing else. `<leader>uc` toggles compact mode, ignoring the
  pointer resting over the sidebar. `<leader>uR` resets the layout.
- `<leader>uz` reader view, `un` dismiss the page's and the window's notifications, `ub` switch light and dark.

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| NAV-01 | P1 | C | **Security.** `p` and `P` load the clipboard text with no scheme check, so `javascript:` and `file:` addresses pass (`browser.rs:741`) | Fix up the text, then open http and https only |
| NAV-02 | P1 | C | `<leader>qs` reopens every tab in the closed-tab list (`tab_actions.rs:504`) | Restore the last session |
| NAV-03 | P1 | T | `<leader>bf` takes the tab out of its folder; there is no picker | Add the "Move to Folder" picker |
| NAV-04 | P1 | T | `<leader>bj` opens a tab search picker | Label the sidebar rows |
| NAV-05 | P1 | C | `<leader>qw` exits the whole application (`browser.rs:773`) | Close the window |
| NAV-06 | P1 | C | `<leader>uz` does nothing (`stub.rs:3443`) | Reader view, once the engine side exists |
| NAV-07 | P2 | C | `<leader>bm` and explorer `m` open the menu at a fixed point, 240 by 96 (`browser.rs:730`) | Anchor it to the row, showing the sidebar |
| NAV-08 | P2 | R | A counted back or forward runs single steps one after another, which may move only once | Jump by the count in one request |
| NAV-09 | P2 | C | The find bar is new UI without the fork's find bar options (highlight all, match case, whole words) | Match the fork's find bar |
| NAV-10 | P2 | C | The `<leader>x` extensions picker differs | See [EXT-05](#extensions) |
| NAV-11 | P3 | C | `un` clears sidebar notifications and toasts; `ub` goes from "system" to dark whatever is showing | Match the fork |
| NAV-12 | P2 | C | `<leader>E` also changes the toolbar layout (`stub.rs:2938`), so the single toolbar is gone after toggling. The fork flips only the sidebar | Change only whether the sidebar is expanded |
| NAV-13 | P1 | T, C | `<leader><Tab>d` deletes the space at once and moves its tabs to the neighbouring space (`stub.rs:1204`). The fork asks first: "Delete Space?", "Are you sure you want to delete { name }? This action cannot be undone." | Confirm first, then do what the fork does with the space and its tabs |
| NAV-14 | P1 | T, C | `<leader><Tab>u` only shows the toast "Unloaded tabs in …"; nothing is unloaded (`stub.rs`, `SpaceUnload`) | Unload the space's tabs |
| NAV-15 | P2 | T, C | `<leader><Tab><Tab>` creates a space at once. The fork opens its new space form | Open the form |
| NAV-16 | P1 | C | `<leader>bM` and sidebar `M` flip the mute icon only; nothing tells the web engine (`handle_tab_toggle_mute`) | Mute the tab's page |
| NAV-17 | P1 | C | `<leader>bu` and sidebar `U` only mark the tab as unloaded and switch away; its page stays loaded (`handle_tab_unload`) | Unload the page, and load it again on activation |
| NAV-18 | P1 | L, T, C | `<leader>,` `<leader>fb` `<leader>fr` `<leader>/` do not filter. The URL bar opens holding a selected `%`, `^` or `?` and offers to search the web for it; typing replaces it, and nothing is restricted | Restrict the URL bar to tabs, history or web search, with no prefix in the query |
| NAV-19 | P1 | C | `<leader><Tab>r` and `<leader><Tab>i` show nothing while the library is closed: the rename field and icon picker are drawn only in the library's Spaces section (`browser.rs:563`) | Show the sidebar and rename or pick there, as the fork does |
| NAV-20 | P0 | L, C | After `<leader><Tab>r` with the library closed, the unseen rename stays on, and the window then stops sending keys to jump labels (`browser.rs:1004`). Letters typed at `s` labels would run vim commands instead | Never keep a rename that is not on screen (fix with NAV-19) |

`-`, `r`, `R`, `yy`, tab close, reopen, pin, essential and duplicate, and moving between spaces are wired and looked
right in the core.

## Jump labels: `s`, `gs`, `S` and `i`

### Behaviour of the fork

Sources: `fork: plugins/ZendiaJumpContent.sys.mjs`, `core/ZendiaJumpLabels.sys.mjs`, `ui/ZendiaHints.sys.mjs`,
`core/ZendiaControls.sys.mjs`.

What each key offers:

| Key | Targets | Labels appear |
| --- | --- | --- |
| `s` | Every control on screen **except fields**, frames included, no bare canvases. Stays open inside a CAPTCHA challenge | At once |
| `gs` | Every control, fields included | After typing some of its text |
| `S` | Every occurrence of the typed text on screen (at most 3000 text nodes, 500 matches) | After the first character |
| `i` | Fields only | At once, when there are several |
| `v`, then `s` in visual mode | Every paragraph and word (at most 560) | At once |

What counts as a control: links with `href`, `button`, non-hidden `input`, `select`, `textarea`, `summary`, the ARIA
roles button, link, checkbox, switch, tab, menuitem, radio, option, combobox, textbox, slider, contenteditable,
`tabindex` other than -1, `onclick`, and any element with a hand or drag cursor its parent lacks. It must be on screen,
at least 4 px, rendered (`checkVisibility()` **with no options**), enabled and uncovered at one of 5 points of its box or
of up to 4 line boxes. A hand inside another control is not counted twice. Open and closed shadow roots are searched.

Labels when shown at once:

- Letters `asdfghlqweruiopzxcvbnmty` (no `j`, no `k`). All labels have one length: 1, then 2, then 3.
- Order: reading order, in rows of 8 px, left to right.
- A typed letter narrows to the labels starting with it and is highlighted in each. A letter starting no label is
  ignored. Capital: open in a new tab (a real Ctrl+click, so page handlers run).
- `<CR>` picks the first target. `<BS>` removes a letter, or ends the jump. `<Esc>` ends it.
- A key with Ctrl, Alt or Super ends the jump **and then runs as usual**.

Labels after text (`gs`, `S`):

- A pattern with a capital matches case; otherwise case is ignored.
- Labels come from `asdfghjklqwertyuiopzxcvbnm`, nearest target first (from the caret for `S`, the middle of the page for
  `gs`). A label is never a character that follows a match, and a target keeps its label while the text grows.
- A key that is a label picks it, once some text is typed; any other key extends the text only if it still matches.
- A prompt shows `› text`.

Picking:

- A field: insert mode, focus, caret at the end.
- Another control: mouse move, down and up at a point where it is on top.
- Text (`S`, words): visual mode starts there, or the selection grows to it; scrolled into view.
- A paragraph: its line is selected, in visual mode.

Lifetime: labels follow scroll (any scroller) and resize on the next frame. A mouse press in the page or leaving the page
ends the jump.

### `i`: choosing a field

1. No count: list the fields on screen, frames included, leaving out covered ones (for example under a dialog).
2. Several: label them. One: focus it at once.
3. None: take the count-th (default first) field in the viewport, even under something transparent.
4. No field at all: insert mode as with `I`, so keys reach the page.

### Label style

All values in CSS pixels. Labels live in the page, so they zoom with it.

| Item | Fork | Port (`ui/vim/page_hints.rs`) |
| --- | --- | --- |
| Control label font | bold 11 on 16, `ui-monospace` | bold 14 on 20 |
| Word and paragraph label font | bold 10 on 14 | bold 11 on 15 |
| Padding | 0 4 (word: 0 3) | 0 5 (word: 0 3) |
| Background, text | `#a25772`, `#eef5ff` | `#a25772`, `#ffffff` |
| Corner, ring | radius 4, no ring | radius 4, 1 px white at 28% |
| Shadow | `0 1px 3px rgba(0,0,0,.4)` | y 1, blur 4, black at 50% |
| Page dim | none | black at 8% |
| Control label place | left + 2, top + 2 of the control | left − 2, centred on the first 28 px of height |
| Collision | move right by width + 2, up to 4 times, when closer than the width and 16 down | move just past the other label, up to 4 times |
| Word label | at the word's left, centred vertically | same, larger |
| Paragraph label | left − width − 8, top of line, `#3d59a1` | left − width − 10 |
| Text match label | at the match's right edge and **top** | right edge, centred vertically |
| Clamping | inside the window, estimating width as 7 per letter + 8 (small 6.5) | inside the viewport |
| Control outline | 1 px `rgba(162,87,114,.55)`, radius 3 | same, alpha .59 |
| Match tint | `rgba(255,199,119,.4)`, radius 3 | same |
| Paragraph bar | 3 wide, 6 left of the line, radius 2, `#3d59a1` | same |
| Typed letters | `#ffc777` behind `#1b1d2b`, flush with the left edge | same, 1 px wider |
| Prompt | bold 13 on 20, padding 2 10, radius 6, `#a25772`, shadow `0 2px 8px`, 16 from the bottom (top if a target is within 52 of the bottom), at most 90% wide | padding 3 12, radius 7, ring, y 2 blur 10 |

The rows in which the port differs are JMP-06. Use the fork column.

### Compared side by side

The same page in both browsers on 2026-11-27 (links in a row, icon buttons, form controls, a card, a shadow root, a
frame and two paragraphs). Counts are targets offered:

| Key | Old | Port | What the port leaves out |
| --- | --- | --- | --- |
| `s` | 20 | 18 | the button in the frame, the invisible (`opacity: 0`) checkbox |
| `i` | 5 | 4 | the field in the frame |
| `gs` then `o` | 12 | 11 | the invisible checkbox |
| `S` then `qu` | 1 | 1 | |
| `v` | 55 | 48 | words on the row of links and on the button, text in the frame |

Seen on screen: the port dims the page, its labels are bigger and sit over the checkbox and radio button so that these
are hidden, and its search prompt is not visible while the agent notice is at the bottom of the window.

### Defects

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| JMP-01 | P0 | C | A pick clicks at CSS position × device pixel ratio, ignoring page zoom (`hub_adapter.rs:616`). The overlay does scale by zoom, so the label looks right and the click lands elsewhere. This also affects the CAPTCHA keys | Multiply by the tab's zoom. Keep zoom per tab |
| JMP-02 | P1 | L, C | Elements with `opacity: 0` are skipped (`automation_world.js:135`, `:291`), so styled checkboxes, radios and file inputs get no label. The fork's check ignores opacity | Use the fork's visibility test for controls and fields |
| JMP-03 | P2 | C | A capital label opens only `<a href>` in a background tab; every other control gets a plain click (`:1393`) | Send a real click with Ctrl held |
| JMP-04 | P2 | C | Labels and the visual cursor move only after a vim scroll or mouse wheel, 180 ms later (`ui/vim/mod.rs:227`). Page scripts scrolling, resizes and layout changes leave them behind | Report scroll and resize from the page |
| JMP-05 | P2 | C | Closed shadow roots are not searched | Needs engine support in the isolated world |
| JMP-06 | P2 | L, C | Labels look and sit differently, and read worse than the fork's (reported from use): a dim over the page, bigger labels that cover small controls, placed 2 left and centred instead of 2 right and 2 down | Match the fork column of the style table |
| JMP-07 | P2 | L | The `› text` prompt of `gs` and `S` is drawn under the agent notice pill at the bottom of the window and cannot be read while the pill shows | Draw the prompt above it, or move one of them |

Also MD-03 (labels survive tab switches), FRM-01 (frames), RT-04 (`i` and type-ahead), CAP-03 (`s` in a challenge).
Label assignment, ordering, `<CR>`, `<BS>`, `<Esc>`, chords and the text-jump rules match.

## CAPTCHA keys

This is the least complete area. The port has a placeholder (`automation_world.js:1416-1461`, marked "the fork's full
CAPTCHA jump is not ported") that looks for elements whose class, id, label or `src` contains words such as `captcha`
or `challenge` in the top document. The large services draw their challenges in cross-origin frames, so `gc` and
`<leader>c…` find nothing on them.

**Requirement (stated 2026-11-27): CAPTCHA control is identical to the fork.** The behaviour comes from the code listed
below, so work from that code, not from screenshots. What follows describes the behaviour to check against; where this
document and the code differ, the code is right. The port follows `Architecture.md`.

### Code to port

| File under `fork:` | Lines | Holds |
| --- | --- | --- |
| `plugins/ZendiaJump.sys.mjs` | 51 | the actions and their descriptions |
| `plugins/ZendiaJumpContent.sys.mjs` | 1953 | label sessions, the staying jump, the CAPTCHA commands, collecting frames |
| `plugins/ZendiaPointerContent.sys.mjs` | 1470 | the keyboard cursor, finding CAPTCHA areas, mouse and touch |
| `core/ZendiaCaptcha.sys.mjs` | 990 | service and widget tables, purposes, solved state, and the pure math |
| `core/ZendiaControls.sys.mjs` | 478 | what is a control, visibility, reading order |
| `core/ZendiaDom.sys.mjs` | 263 | editable fields, shadow DOM, key listeners |
| `core/ZendiaKeys.sys.mjs` | 426 | key names |
| `core/ZendiaJumpLabels.sys.mjs` | 79 | label letters and assignment |
| `core/ZendiaPageText.sys.mjs` | 179 | page text for text jumps |
| `ui/ZendiaHints.sys.mjs`, `ui/ZendiaPicker.sys.mjs` | 112, 273 | imported by the jump and the cursor |
| `plugins/ZendiaVisualContent.sys.mjs`, `core/ZendiaTextObjects.sys.mjs` | 375, 373 | imported by the jump for words and visual mode |
| `captcha/ZendiaCaptchaWatchChild.sys.mjs` | 2397 | the cursor's finder loads it for `ownChallenges` only |
| `actors/ZendiaKeysChild.sys.mjs`, `ZendiaKeysParent.sys.mjs` | 841, 198 | not imported, but they hand keys to the session, from frames too |

Tests that come with it, in `~/zendia/src/zen/tests/zendia/`: `test_captcha.js`, `test_jump_labels.js`,
`browser_zendia_captcha.js` and its `file_captcha_*.html` pages.

`ZendiaCaptcha.sys.mjs` and `ZendiaJumpLabels.sys.mjs` make no Firefox-only calls, so only their loading has to change.
The calls below were found by searching those two, JumpContent, PointerContent, Controls and Dom. The other files were
not searched; check every file while porting:

| Firefox-only, or differing | Used in | What the port needs |
| --- | --- | --- |
| `browsingContext`, `sendQuery` between frames | JumpContent, PointerContent, Controls, Dom | the script in every frame, and messages between them (FRM-01) |
| `synthesizeMouseEvent`, `synthesizeTouchEvent` | JumpContent, PointerContent | real mouse and touch input from the browser: move, press, release, drag |
| `openOrClosedShadowRoot` | PointerContent, Dom | closed shadow roots from the engine (JMP-05) |
| `windowUtils.nodesFromRect` | Controls | hit testing a rectangle |
| `checkVisibility(…)` | JumpContent, PointerContent, Controls | a standard call; pass the same options (JMP-02) |
| `setUserInput` | JumpContent (`captcha.fill`) | setting a value as typing does |
| `ownerGlobal`, `documentGlobal` | JumpContent, PointerContent, Controls | the element's window |
| `Services.els` system listeners, `isTrusted` | Dom | listeners a page cannot stop, real events told from scripted ones |
| `resource:///` imports, `ChromeUtils` module getters | the importing files | the modules bundled into the one page script |

### Keys

| Where | Key | Does |
| --- | --- | --- |
| Normal | `gc`, `<leader>cc` | Labels the CAPTCHA only, and stays |
| Normal | `<leader>cv` | Presses verify (Verify, Next, Skip, Submit), without labels |
| Normal | `<leader>ci` | Focuses the answer field, insert mode |
| Normal | `<leader>cr` | New challenge |
| Normal | `<leader>ca` | Audio challenge, or back to images from it |
| Normal | `<leader>cp` | Plays the audio |
| In the labels | letters | Pick; the labels stay |
| In the labels | capital last letter | Hold that target and drag it |
| In the labels | `<CR>` / `<Tab>` | Verify / answer field |
| In the labels | `<C-r>` / `<C-a>` / `<C-p>` | New challenge / audio or images / play |
| In the labels | `.` / `,` | Cursor to click anywhere / to drag from anywhere |
| In the labels | `<BS>`, `<Esc>` | Take back a letter / leave |
| In `s` labels, before any pick | `.` / `,` | The same cursor, over the CAPTCHA, or over the whole page if there is none |
| In the cursor | `<C-r>` / `<C-a>` / `<C-p>` | Close the cursor, then new challenge / audio or images / play |

- A `<leader>c…` key from normal mode with nothing to press does nothing and shows no message.
- After a `<leader>c…` press that did not focus a field, the top page takes the focus again so the keys keep working.
- `gc` pressed with the focus in a frame starts in the top page and takes the focus out of the frame.
- While a jump stays in a challenge, Ctrl, Alt and Super keys other than the ones above are swallowed. In any other
  jump they end it and run.

### The staying jump

- Targets are only parts of a CAPTCHA. A frame's controls count when the frame is a CAPTCHA's by its address. Pictures
  nothing listens to (an image or canvas of at least 100 by 60) become targets too, for pointing at.
- Picking leaves the labels up. After a click the page is read again in 350 ms, then every 900 ms.
- A plain pick does not always click. A slider handle or other thing known to be dragged, and a piece on a board with
  empty places, is **held** and the cursor opens (over the pictures and the target, when there are pictures). A picture
  opens the cursor over it alone. Everything else is clicked, and a field takes the focus.
- A pick from `s` outside a challenge ends the jump, as on any page.
- Labels are **sticky**: a target keeps its label while it exists; new targets take free ones of the same length.
- It ends on `<Esc>`, when the CAPTCHA is solved (an answer field gains a value, or a widget shows a success mark), or
  after 3 reads in a row with no targets. Once no labels remain, the next key is the user's again.
- The same staying behaviour starts from `s` when the pick is inside a challenge; the labels already seen keep their
  letters.
- While switching, keys are ignored, so nobody picks from labels about to change.
- Picking a field gives it the keys; the jump waits and takes them back after `<Esc>`.
- Keys typed in any frame of the page go to the labels.
- A click does not end it. Labels follow scroll and resize.
- Prompt, at the bottom (top if a target is there):
  `captcha · label picks · Label holds · Enter verify · Tab answer · ^R new · ^A audio · ^P play · . cursor · , drag · Esc leave`
- A key with nothing to press flashes for 1500 ms: `no verify button here`, `no answer field here`,
  `no new-challenge button here`, `no audio challenge here`, `no play button here`. Before saying so, the page is read
  once more, as a button may have just been enabled.
- If there is nothing to label, the cursor opens over the CAPTCHA. If none is found, over the whole page with
  `no CAPTCHA found: the cursor is on the page`.
- Agents call `jump.captcha` with `orPage`: with no CAPTCHA it labels the page as `s` does, and it returns which of the
  two it labeled.

### What each control is for

Rules are tried in this order against the label (`aria-label`, title, text), then against the id and class with `_` read
as a space. Labels or names matching `download|help|privacy|terms|report|feedback|logo` are never pressed. Checkboxes
and radios have no purpose.

| Purpose | Matches |
| --- | --- |
| answer | any editable control that is not a `select`; for `input`, types text, search, tel, number, email, url or none |
| play | `\bplay\b`, `\breplay\b`, `listen again`, `play-?button` |
| visual | `\bvisual\b`, `image-?button`, `get an? (?:image\|picture)`, `(?:image\|picture) challenge instead` |
| audio | `\baudio\b`, `\blisten\b`, `headphone`, `\bsound\b`, `\bvoice\b` |
| reload | `reload`, `refresh`, `new challenge`, `another challenge`, `try another`, `try again`, `start over`, `restart`, `^reset\b`, `new (?:image\|picture\|puzzle\|question)s?\b` |
| verify | `\bverify\b`, `\bsubmit\b`, `^next\b`, `\bskip\b`, `^check\b`, `\bconfirm\b`, `^done\b`, `^continue\b`, `^ok\b`, `verify-?button`, `button-?submit` |

### Recognising a CAPTCHA

- **Frames by address** (`ZendiaCaptcha.sys.mjs:60-162`): reCAPTCHA, hCaptcha, Cloudflare Turnstile, Arkose, GeeTest,
  HUMAN, DataDome, Yandex SmartCaptcha, Tencent, NetEase Yidun, MTCaptcha, Friendly Captcha, AWS WAF, Lemin,
  KeyCAPTCHA. Each entry says which of its frames asks for clicks (for example reCAPTCHA `/bframe`, hCaptcha
  `frame=challenge`). The services' own websites are excluded.
- **Widgets in the page** (`:164-199`): selectors such as `.g-recaptcha`, `.h-captcha`, `.cf-turnstile`, `.frc-captcha`,
  `[class*='geetest_']`, `[class*='yidun']`, `#px-captcha`, `#challenge-form` (full list in the file).
- **A site's own CAPTCHA** (`:352-354`, `ZendiaPointerContent.sys.mjs:317-458`): a picture or field whose attributes
  name a captcha, security or verification code (not a logo or icon), taken with the smallest box holding both and its
  nearby buttons.
- **Challenges drawn by the page** (`:366-420`): short texts such as "Select all images…", "Slide the piece…", "I'm not
  a robot". The finder is `ownChallenges` in `captcha/ZendiaCaptchaWatchChild.sys.mjs`; the jump depends on that one
  function of the watcher.
- **Solved** (`:316-332`, `:483-484`): a filled answer field (`g-recaptcha-response`, `h-captcha-response`,
  `cf-turnstile-response` and the rest), or a widget with a success, verified, solved or passed class.

### The keyboard cursor

For pictures, sliders, pieces and hold buttons (`ZendiaPointerContent.sys.mjs:22-102`, `:750-1470`).

- Layouts, cycled with `<Tab>` (then hidden): a **board** (places inferred from the pieces' lattice), a **ruler** (stops
  along a slider's track, drawn on the piece's row), a **grid** (cells of about 40 px, grown until two letters suffice).
- Cursor letters: `asdfghlqweruiopzxcvbnmty` without `h j k l v`. A label moves there; a capital clicks or drops; on a
  board any label drops. After `,`, the first label takes hold.
- Moving: `h j k l` 2 px, `H J K L` 10, arrows 1, shifted arrows 10, times a typed count (at most 999).
- `<Space>` clicks, or drops. `<CR>` verifies when possible, else clicks. `v` holds and releases.
- `<Esc>` clears typed letters or the count; while holding, returns to the start and releases; else back to the labels.
- `<BS>` takes back a typed letter, or else clears the count.
- Moves are paths: hover steps of 36 px, drag steps of 12 px with 8 ms pauses; drags send touch events too; the button
  is never left down, and events follow into frames.
- A cursor that began by holding ends when it lets go. It also ends when the CAPTCHA is gone (checked 700 ms after a
  click, then every 900 ms, 3 misses).
- Drawing: areas outlined 2 px dashed `rgba(255,199,119,.8)`; cells 1 px `rgba(162,87,114,.55)`; tags bold 10 on 13,
  `#232136` with `#eef5ff`, typed letters `#ffc777`; ruler lines 1 px, letters on alternating rows 14 apart; the cursor
  is an arrow with a ring of 16 (22 holding, filled `rgba(255,199,119,.45)`).
- Prompt (bold 13 on 20, `#232136`), parts joined with ` · `:
  holding: `slide · letter puts the piece on its line`, or `holding · label puts it there`, or
  `holding · label drags there · Label drops`; then `hjkl HJKL ←→ move`, `Space/Enter drop`, `Esc undo`.
  Not holding: `drag · label takes hold there` or `cursor · label moves · Label clicks`; then `hjkl HJKL ←→ move`,
  `Space click`, `v hold`, `Enter verify` (if possible), `Esc labels`. Always `Tab <next layout>` or `Tab hide`, and a
  `click` flash for 600 ms.

The math behind sticky labels, rulers, grids, boards, counts and paths has no DOM in it
(`ZendiaCaptcha.sys.mjs:756-989`, tested by `test_captcha.js`). Bring it over first, with its tests.

### Run on the old fork's test pages

Both browsers, 2026-11-27, on the fork's own CAPTCHA pages. The tiles and slider challenges sit in a frame of another
origin; the board is the page itself. All are recognised by a service's name in the address. Results are what the pages
reported. These are stand-ins; no real service was tried.

| Page and keys | Old | Port |
| --- | --- | --- |
| Tiles: `gc` | Labels the 3 tiles and verify (`a s d f`) and shows the prompt above | Labels not recorded |
| `a`, `s` | Press two tiles; the labels stay | No tile pressed |
| `<Tab>`, `<C-p>` | "no answer field here", "no play button here" | Messages not recorded |
| `<CR>` | Verifies, and the jump ends | Not verified |
| Tiles: `<leader>cv` | Verifies without labels | Not verified |
| Slider: `gc`, `A` | Holds the handle; a ruler of 21 labels along the track | Handle not held |
| `9L`, `l`, `<Space>` | Moves 90 px, then 2 px, drops at 92; the labels return, the handle keeps `a` | Not moved |
| `gc`, `.`, `a`, `<Space>` | A grid of two-letter labels; clicks the picture under the cursor | No click |
| Board: `gc`, `a`, `s` | Takes a piece, labels 9 places, puts it on the second; the labels return | No piece moved |

The port's labels and messages are drawn by the window, so the pages could not report them; its column is about the
pages' state.

### Defects

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| CAP-01 | P1 | L, C | No frames, so a CAPTCHA in a frame is not reached: on the framed test pages no CAPTCHA key changed anything (needs FRM-01) | Frames first |
| CAP-02 | P1 | C | None of the recognition rules: services, widgets, own and drawn CAPTCHAs, solved state | Port `ZendiaCaptcha.sys.mjs` |
| CAP-03 | P1 | C | No staying jump: a pick ends it, from `gc` and from `s` alike; no sticky labels, re-reads or end conditions | Port the fork's `JumpSession` |
| CAP-04 | P1 | C | None of `<CR>` `<Tab>` `<C-r>` `<C-a>` `<C-p>` in the labels. `<CR>` picks the first target, `<Tab>` is ignored, the Ctrl keys end the jump | Add them with the flash messages |
| CAP-05 | P1 | C | No cursor: `.`, `,` (in `s` too), and the fallback when nothing can be labeled | Needs hover, button-held and drag mouse input through the Hub |
| CAP-06 | P1 | C | No holding or dragging: slider, ruler, board, piece, press-and-hold | Part of the cursor |
| CAP-07 | P1 | L, C | `<leader>c…` match loose words in a few elements. `ci` takes the first field of any kind, `ca` never switches back, and `cv` may press any "submit" or "continue" button inside a box whose class mentions a challenge | Use the purpose rules, only inside recognised CAPTCHAs |
| CAP-08 | P1 | C | A picked answer field does not return the keys to the labels on `<Esc>` | Part of the staying jump |
| CAP-09 | P2 | C | None of the prompts or messages | Copy the strings above |

JMP-01 (zoom) breaks CAPTCHA clicks too. The fork's `captcha.fill` and agent-driven jumps belong to the agent work.

## Visual mode and text objects

Fork behaviour (`fork: plugins/ZendiaVisualContent.sys.mjs`, `core/ZendiaTextObjects.sys.mjs`, `ZendiaPageText.sys.mjs`):

- `v`: keep an on-screen selection, else put the cursor at the first visible text. If collapsed, label every paragraph
  and word; `<Esc>` there keeps the cursor.
- A 2 px `#fca7ea` bar marks the moving end and follows scroll and resize.
- `h j k l` and arrows, `w` `e` `b`, `0` `^` `$`, `{` `}`, `gg` `G` (scrolling to the end), all counted except line
  ends and page ends. `o` swaps ends. `<C-d> <C-u> <C-f> <C-b>` scroll. `s` and `S` extend by label. `?` which-key.
- Objects `a`/`i` with `( ) b [ ] { } B < > " ' ` q w W e d s p t g _ . , ; : + - = ~ | / \ & $ #`, within 50 lines,
  counted, growing when repeated.
- `y`: copy as a normal copy (HTML and text), flash `rgba(255,199,119,.55)` for 300 ms, cursor to the start, normal mode.
- `v` and `<Esc>` leave, collapsing at the cursor. Clicking into a field ends visual mode.

The motions, text objects, `o`, land labels, yank flash and cursor color match. Ladybird supports the selection motions
they use.

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| VIS-01 | P2 | C | The cursor is scrolled into view only through the focused element's scroller or the document (`automation_world.js:1060`), so it can leave sight inside other scrolling boxes | Scroll every ancestor |
| VIS-02 | P2 | C | `y` copies plain text only (`:1231`) | Copy as the page's own copy does |
| VIS-03 | P3 | C | Text hidden by opacity is left out of words and text objects, which the fork included | Match, unless the word filter is kept |
| VIS-04 | P2 | L | `v` offers no word on a row of links written without spaces, nor on a button's text. The fork labeled both. The port's word filter is the likely cause | Offer them as the fork does |

Also MD-01, MD-02, JMP-04, FRM-01.

## Yank ring

Fork behaviour (`fork: core/ZendiaYankRing.sys.mjs`, `plugins/ZendiaYank*.sys.mjs`):

- 100 entries, newest first, no duplicates, skipping blank text and text over 100,000 characters. Saved to
  `<profile>/zendia-yank.json`, 2 s after a change.
- Every copy and cut in a page joins it, **never from a password field and never in a private window**. Opening the
  history also adds the system clipboard.
- `<leader>p`: picker "Yank History"; each entry is its first non-blank line (cut at 100) with `host · age`
  (`now`, `5m`, `3h`, `2d`). Empty: toast "Nothing yanked yet." A pick goes into the focused field, else to the
  clipboard with "Copied to the clipboard."
- `<A-p>` `<A-n>` in a field: right after a put, replace it with the older or newer entry; otherwise put the newest.
- A put flashes the field `rgba(255,199,119,.45)`, radius 3, for 300 ms. `yank.clear` toasts "Yank history cleared."

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| YNK-01 | P1 | C | Kept in memory only | Persist in the profile |
| YNK-02 | P1 | C | Ctrl+C, cut and menu copies in pages do not join the ring | Report page copies |
| YNK-03 | P1 | C | The system clipboard is not added when the history opens | Add it |
| YNK-04 | P1 | C | No password or private-window rule. Required **before** YNK-02 and YNK-03 | Exclude both |
| YNK-05 | P3 | C | Entries show the host without the age; the put flash uses the yank color (.55) and radius 2 | Match |

## Explorer (sidebar)

Fork behaviour (`fork: plugins/ZendiaExplorerSpec.sys.mjs`, `ui/ZendiaExplorer.sys.mjs`):

While the explorer or toolbar is open, a hidden compact sidebar stays out, also when one hands over to the other;
closing hides it again, on the empty tab too. (The port reveals it.)

Rows are essentials, pinned tabs and folders, then tabs, exactly as shown: nothing from a collapsed section or folder,
a split as one row. Opening shows a hidden compact sidebar and puts the cursor on the current tab. The cursor follows
tab changes. "Targets" means the selected rows, or the row under the cursor, and an action on targets clears the
selection.

| Key | Fork | Port now |
| --- | --- | --- |
| `j` `k`, arrows | Move, counted | Same |
| `gg` `G` | First, last; a count picks that row | Count ignored (EXP-13) |
| `<C-d>` `<C-u>` | Half the visible rows | Always 8 (EXP-15) |
| `<CR>` | Tab: select and close. Folder: open or close it. Footer: press | Split selects its first tab (EXP-19) |
| `l` | Footer: move right. Otherwise the same as `<CR>` | **Nothing on a tab** (EXP-02) |
| `o` | As `<CR>` but the explorer stays | Tabs only (EXP-09) |
| `h` | Footer: move left. Open folder: close it. Inside a folder: close the folder | Moves to the folder only (EXP-03) |
| `za` `zo` `zc` | Toggle, open, close the row's folder; cursor moves to it | Cursor stays (EXP-16) |
| `zR` `zM` `Z` | Open all, close all | Wired |
| `s` | Label rows on screen; **move** the cursor there | Opens the tab and leaves (EXP-06) |
| `/` | Close, search tabs in the URL bar | Other picker, explorer stays (EXP-08) |
| `a` | Close, new tab | Explorer stays (EXP-08) |
| `A` | New folder, renaming it in place | No rename (EXP-11) |
| `d` | Delete selected tabs; with no selection on a folder, delete the folder | **Closes the active tab** (EXP-01) |
| `c` `p` `e` `R` `U` `M` | Duplicate, pin, essential, reload, unload, mute the targets | **Act on the active tab** (EXP-01) |
| `r` | Rename the row, tab or folder; keys return afterwards | Active tab (EXP-01) |
| `y` `Y` | Copy addresses or Markdown links of the targets, silently | Toast, selection kept (EXP-17) |
| `>` | Picker "Move to Folder"; a new folder is renamed | **Removes the active tab from its folder** (EXP-10) |
| `<` | Move targets out of their folder | Same |
| `<A-j>` `<A-k>` | Move the row (tab, split or folder) within its section, counted | First tab only, switches tabs on the way (EXP-18) |
| `<C-v>` `<C-s>` | Split the current tab with the row's tab, close | First target only (EXP-18) |
| `m` | The row's tab or folder menu, on the row | Tabs only, fixed place (EXP-11) |
| `+` | The sidebar's create menu (space, folder, split, tab) | Picker with tab and folder (EXP-11) |
| `<Tab>` | Select the row, move down | Same |
| `V` | Select a range, added to `<Tab>` picks | Replaces the picks (EXP-14) |
| `u` | Reopen closed tab | Same |
| `q`, `<leader>e` | Close | Clear the selection first (EXP-12) |
| `<Esc>` | Clear the selection, else close | Same |
| `gt` `gT`, `?`, `<leader>…` | Spaces, which-key, leader maps | Same |

Footer: `j` past the last row moves to the buttons at the sidebar's foot (library, spaces, +), at the button last used.
There `h` `l` move across, `<CR>` and `o` press (`+` opens the create menu), `k` returns to the rows, `j` does nothing.
Not available during a range selection.

Highlight: cursor 2 px inner outline in the explorer color; selected rows 2 px visual color plus 25% fill; footer
cursor 2 px outline, 30% fill, radius 6. Row labels: bold 11 on 16, padding 0 5, radius 4, 4 from the right, centred.
The statusline shows VISUAL while a selection exists.

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| EXP-01 | P0 | L, T | `d c p e M U R r` act on the active tab (`resolve_target_tab`) | Explorer commands that take the targets; `explorer_target_tabs` in `stub/chrome_actions.rs` already computes them |
| EXP-02 | P1 | L, T | `l` does nothing on a tab (`commands.rs:2483`, `focus.rs:492`). On a folder it steps into it instead of toggling | Footer move, else the `<CR>` action |
| EXP-03 | P1 | T | `h` inside a folder does not close it; on the footer it does nothing | Match the table |
| EXP-04 | P1 | T | The footer is part of the row list, walked with `j` `k` | Footer rules above |
| EXP-05 | P1 | T | The cursor does not follow tab changes, and jumps to the top after its tab closes | Follow activation; keep the index |
| EXP-06 | P1 | T | `s` activates the picked row and leaves. It also labels off-screen rows and footer buttons | Move the cursor only; label visible rows |
| EXP-08 | P1 | T | `a` and `/` leave the explorer open; `/` searches differently | Close first; URL bar tab search |
| EXP-09 | P1 | C | `o` ignores folders and footer buttons | Match |
| EXP-10 | P1 | T | `>` has no folder picker | Add it (NAV-03) |
| EXP-11 | P1 | C | Menus: no folder menu, no folder rename with `r` or after `A`, tab menu at a fixed place, a two-item create picker | Row-anchored menus, inline rename, full create menu |
| EXP-12 | P2 | T | `q` and `<leader>e` need two presses after a selection | Close at once |
| EXP-13 | P2 | T | `gg` and `G` ignore counts | Pass the count |
| EXP-14 | P2 | T | `V` drops `<Tab>` picks | Union of both |
| EXP-15 | P2 | C | Half page is a fixed 8 | Half of the visible rows |
| EXP-16 | P2 | C | The cursor walks hidden rows: a collapsed pinned section (the core does not know it is collapsed, `sidebar/mod.rs:541`) and folders inside a collapsed folder. After `zc` it stays on a hidden row | Core knows what is shown |
| EXP-17 | P2 | C | Yank adds a toast and keeps the selection; other actions keep it too | Silent; clear the selection |
| EXP-18 | P2 | C | `<A-j>` `<A-k>` activate the tab temporarily and move one tab only; splits use the first target | Move the row without activating |
| EXP-19 | P2 | C | A split row selects its first tab | Last used pane |
| EXP-20 | P3 | C | Row labels fade the typed letter; the fork hides the labels that no longer match | Match |

## Toolbar mode

Fork behaviour (`fork: ui/ZendiaToolbar.sys.mjs`): the items are the visible top buttons, the URL bar, the main menu,
the chat button and the window buttons, in reading order (rows of 16 px). `<leader>m` starts on the **main menu
button** and shows a hidden compact bar. `h` `l` (counted) and arrows move, `0` `^` first, `$` last, `<CR>` `o` press
and leave (the bar stays for a menu it opens), `m` opens the **item's** context menu (history on back and forward; leave
only if an entry was picked), `s` labels items and **moves** the cursor, `i` leaves and selects the URL bar. Cursor:
2 px inner outline in the toolbar color, radius 6, a pill on the URL bar.

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| TB-01 | P2 | T | Starts on the URL bar | Start on the main menu button |
| TB-02 | P1 | C | `s` presses the labeled item | Move the cursor only |
| TB-03 | P1 | C | `m` opens the app menu | The item's context menu |
| TB-04 | P1 | T | `i` focuses the URL bar but toolbar mode stays on | Leave the mode first |
| TB-05 | P1 | C | An extension button's popup opens without labels | See [EXT-01](#extensions) |

## Splits, panels and window mode

Fork behaviour (`fork: plugins/ZendiaWindows.sys.mjs`, `ZendiaPanes.sys.mjs`, `ui/ZendiaWindowMode.sys.mjs`,
`~/zendia/src/zen/split-view/ZenViewSplitter.mjs`):

Creating and arranging:

- `<leader>|` and `w v`: split right; `<leader>-` and `w s`: split below. A picker ("Split Window Right" or "Split
  Window Below") offers "Open URL…" (detail "any website", key `/`) and the space's tabs that are in no split, with
  host and icon. With no candidates, the URL bar opens at once. Only the **current pane** is divided; the new pane gets
  the focus. At most 4 panes: "Can't add more panels to the split view!". Mixing pinned and unpinned pins the new tab:
  "Tab pinned to join this split."
- `wa`, `w a`: picker "Add to Split", then **carry** the tab.
- `wh wj wk wl` focus the pane sharing that edge (most overlap, then last used). `ww` next in reading order, `wp` last
  used, `wf` a label on each pane chip.
- On the empty tab, splitting is refused. A picker's choice is dropped if the selected tab changed meanwhile.
- Window mode numbers the panes on their chips and closes on a click outside the chips and menus. Picking a pane up
  needs at least two panes.
- `wt` moves the pane to its own tab and focuses the last used pane. `wo` unsplits. `wm` makes the pane fill the view,
  again to undo. `w=` equalize. `wr` `wR` rotate.
- `w+ w- w> w<`, `<C-arrows>`: resize by 5% times the count, minimum pane 7%.
- **Preview before moving:** `wg` picks up the pane over the last used one; `wx` does the same for a swap; `wH wJ wK wL`
  pick it up at that edge. While carrying, `h j k l` move the landing place, `H J K L` jump to an edge, `f` picks a pane,
  `<CR>` drops, `<Esc>` cancels (the mode stays). Labels: "Move here", "Swap", "Move to left edge", "Move to right
  edge", "Move to top", "Move to bottom"; when adding: "Split left", "Split right", "Split above", "Split below",
  "Split at left edge", "Split at right edge", "Split at top", "Split at bottom". A mode opened only to carry closes
  after the drop.
- Outside window mode, a changed pane's chip opens for 1.2 s.

Sizes, closing and the empty pane:

- Resizing changes the nearest row or column around the pane: it trades space with the next pane there, else the one
  before. A pane in a row has no height of its own to change, and the reverse.
- Swapping and rotating keep the places' sizes. Removing a pane keeps the others' shares.
- Picking another pane of a zoomed split ends the zoom.
- Closing a pane's **tab** stays in the split: the first remaining pane is selected.
- With four panes, nothing is asked and nothing is opened: only the limit toast shows.
- `wt` and `wo` on an empty pane (opened for a URL) drop it and return to the tab that was split.
- The sidebar's `<C-v>` `<C-s>` divide the current pane, adding to the split on screen.

Panels (agents, agent, web chat):

- The row is: left panels, the page, right panels. `<C-h>` `<C-l>` move along it; from the page, split panes come first.
  `<C-j>` `<C-k>` do nothing in a panel. `<C-\>` goes back to where the keys were (page or panel), in normal and insert
  mode. At the end of the row the key is swallowed, never a browser shortcut. In a panel's text field `<C-h>` deletes.

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| WIN-01 | P1 | L, T | **`<leader>\|` and `<leader>-` do not split** (also `w v`, `w s`): met in use and reproduced. The keys arrive, but the profile maps them to `split.set_layout`, which only turns a split that exists, so on an unsplit tab it fails with "split group not found". In a split they turn the whole layout | Map them to a new split command that opens the split picker on any tab and divides only the current pane: `\|` and `v` right, `-` and `s` below |
| WIN-02 | P1 | C | `wa` opens an empty tab beside the page | Picker, then carry |
| WIN-03 | P1 | T | `wx` and `wH wJ wK wL` move at once | Preview, `<CR>`, `<Esc>` |
| WIN-04 | P1 | T | `<Esc>` while carrying leaves window mode (`commands.rs:2495`) | Cancel the carry; `q` leaves |
| WIN-05 | P1 | C | `wf` opens a list picker "Pick Window" | Labels on the chips |
| WIN-06 | P1 | C | The panel row does not exist: `<C-h/j/k/l>` never reach panels, `<C-\>` only swaps split panes | Build the panel row |
| WIN-07 | P2 | T, C | `wg` stays in window mode after the drop and toggles the carry off when repeated | Fork lifecycle |
| WIN-08 | P2 | C | The neighbour pane is chosen by centre distance (`split_actions.rs:331`) | Shared edge, overlap, last used |
| WIN-09 | P2 | T, C | Zoom squeezes the other panes; the fork hides them | Fill the view |
| WIN-10 | P2 | C | Flat layouts: no dividing one pane, no mixed nesting, and no pinning or pinned toast when pinned and unpinned tabs are mixed | A layout tree |
| WIN-11 | P2 | C | The chip's zoom button only focuses (`window_mode.rs:139`); the 1.2 s chip flash is missing | Fix both |
| WIN-12 | P1 | T | The sidebar's `<C-v>` `<C-s>` with a split on screen **throws a pane out**: split A B plus C becomes A C (`handle_explorer_split`) | Add to the split by dividing the current pane |
| WIN-13 | P1 | T | `wH wJ wK wL` do not move the pane to the edge. From the top-left of three panes, `wH` and `wK` do nothing, and `wJ` and `wL` only swap with the neighbour (`move_pane_layout`) | Move it to that edge, over the whole height or width |
| WIN-14 | P1 | L, T | A zoom stays when the focus moves: after `wm`, `ww` leaves the keys in a pane squeezed to 15% | End the zoom when another pane is picked |
| WIN-15 | P1 | T | `<leader>bd` on a pane leaves the split for an unrelated tab | Stay in the split |
| WIN-16 | P1 | T | With four panes, `wa` shows the limit toast **and** leaves a new blank tab behind | Check the limit before opening anything |
| WIN-17 | P2 | T | Swapping, rotating, moving to an edge and removing a pane reset every pane to equal size | Keep the sizes as the fork does |
| WIN-18 | P2 | T | A pane cannot shrink below 15% (`split.rs:19`). The fork's limit is 7% | Use 7% |
| WIN-19 | P2 | L, T | `wt` leaves the keys on the tab it removed, now shown alone | Focus the pane used last |
| WIN-20 | P2 | C | An empty pane is not dropped: no cleanup when the URL bar closes, and `wt` `wo` keep the blank tab | Drop it and return to the tab that was split |
| WIN-21 | P2 | C | A key window mode has no use for ends the mode but is not run as a normal mode key; the page just gets the key (`ui/vim/mod.rs:212`) | Handle it again in normal mode |
| WIN-22 | P3 | T | `wg` without a split enters window mode with nothing to pick up | Do nothing |
| WIN-23 | P3 | C | `ww` follows the stored order, which in a grid is not reading order | Rows from the top, each from the left |

## Library mode

Sections in the rail: Routing, Media, Downloads, Spaces, History (Boosts is excluded).

Fork behaviour (`fork: ui/ZendiaLibraryView.sys.mjs`, `plugins/ZendiaLibrary.sys.mjs`):

- Every section with a search box (History, Downloads, Media) **opens with the keys in it**, as insert mode.
- In the search box: `<C-j>` `<C-n>` `<Down>` and `<C-k>` `<C-p>` `<Up>` move the list; `<CR>` opens the item;
  `<Tab>` `<S-Tab>` switch section and keep the search focused; `<Esc>` moves to the list.
- A click on the page hands it the keys; a click in the library takes them back. Leaving a field returns to the list.
- The cursor walks the list, the media grid's columns, one column per space (header, tabs, folders, then the add
  button), or the filter panel's buttons and fields when it is open. A new section or search starts at the top.
- In the media preview, `h` `l` step and `q` closes.

| Key in the list | History | Downloads | Media | Spaces | Routing |
| --- | --- | --- | --- | --- | --- |
| `<CR>` `o` | open | open | preview | header: close library and switch to the space; tab: open | edit |
| `d` | forget the visit | remove from list, keep file | | close the tab | delete route |
| `y` | copy address | copy link | copy | | copy link |
| `m` | row menu | row menu | row menu | tab menu, or space actions on a header | row menu |
| `a` | | | | add space | add route |
| `r` `t` `I` | | | | rename, theme, icon of the **cursor's** space | |
| `<A-h>` `<A-l>` | | | | move the cursor's space, counted | |
| `f` | filters | filters | filters | | |

Everywhere: `j k h l` and arrows (counted), `gg` `G`, `<C-d>` `<C-u>` (half the visible rows), `/` `i` search, `s` label
the items on screen and move there, `<Tab>` `<S-Tab>` sections (counted, keys stay in the list), `gh gd gs gm gr` go to
a section, `?` which-key, `q` `<Esc>` leave the filter panel or form, else close. From a page, `<leader>oo` toggles and
`<leader>oh od os om or` toggle that section.

Cursor: 2 px inner outline in the library color; list rows also 18% fill.

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| LIB-01 | P1 | T | `r t I <A-h> <A-l>` act on the **active** space from any section, even with the library closed; `a` always opens the new space form (`chrome_actions.rs:52`) | Spaces section and cursor's space only; `a` adds a route in Routing |
| LIB-02 | P1 | C | The library's own handler takes `j k h l g G o / i f q d y` even in the middle of a `<leader>` sequence (`library/nav.rs:382`): `<leader>oo` opens the item under the cursor, `<leader>qq` closes the library | Send keys through the engine and have it drive the cursor |
| LIB-03 | P1 | C | `s` and `m` do nothing (`browser.rs:807` ignores them) | Implement |
| LIB-04 | P1 | C | `d` and `y` work on History only (`nav.rs:612`) | Per section, as the table |
| LIB-05 | P1 | C | With the filter panel open, `j k` move the hidden list; filters cannot be reached | Cursor rows for the panel and the routing form |
| LIB-06 | P1 | C | Counts do not work: digits go to the engine, motions bypass it, and the stale count applies to a later key | Fixed with LIB-02 |
| LIB-07 | P1 | C | Downloads and Media open without search focus. While typing in a search box the statusline says LIBRARY, not INSERT | Focus search on open; report insert |
| LIB-08 | P2 | C | Half page is a fixed 6 rows (3 grid rows) | Half the visible rows |
| LIB-09 | P2 | C | `<leader>oh od os om or` open but do not close an open section | Toggle |
| LIB-10 | P2 | C | No which-key popup after `g` | Fixed with LIB-02 |
| LIB-11 | P2 | R | `<CR>` on a space header may leave the library open | Close, then switch |

`<Tab>` order, filter close on `<Esc>` and the preview keys match.

## Pickers

Fork (`fork: ui/ZendiaPicker.sys.mjs`): labels `asdfghlqweruiopzxcvbnmty` (items may reserve a key); `j` `<Down>`
`<Right>` `<C-n>` next, `k` `<Up>` `<Left>` `<C-p>` previous, wrapping; `<CR>` picks; a label picks (case matters);
`<Esc>` `<C-c>` `<C-[>` or an outside click cancel; every other key is swallowed. Footer
`j/k move · Enter pick · label picks · Esc cancel`, in grids too. Grid: 3 columns.

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| PIC-01 | P2 | C | Grids move with `h j k l`, drop `h` `l` from the labels and show another footer | Fork keys in grids |
| PIC-02 | P3 | C | `/` opens a filter that the fork does not have (`vim/picker.rs:578`) | Remove, or approve |
| PIC-03 | P3 | C | The grid's highlight does not glide from tile to tile | Move the highlight as the fork does |

## Which-key

Fork (`fork: ui/ZendiaWhichKey.sys.mjs`): shows 200 ms after a prefix and updates at once while visible; bottom corner
away from the sidebar; keys shown as `SPC TAB RET ESC BS <`; mappings before groups; title `SPC » b +buffer`; footer
`ESC close · BS back`; `<leader>?` (and `?` in other modes) shows "Keymaps" until the next key. The port matches this.
Its **descriptions** differ:

| Keys | Fork text | Port text |
| --- | --- | --- |
| `i <A-p>`, `<A-n>` | Put Previous Yank, Put Next Yank | Cycle Previous Yank, Cycle Next Yank |
| `<leader>bP` | Delete Non-Pinned Tabs | Delete Unpinned Tabs |
| `<leader>fn` | New Tab | New Tab (URL bar) |
| `<leader>ff` | Find Tabs, History and Bookmarks | Open URL Bar (see decisions) |
| `yy` | Copy URL | Yank Page URL |
| `<leader><Tab>i` | Change Space Icon | Space Icon |
| `ww`, `wp` | Go to Next Window, Go to Last Used Window | Next Window, Last Window |
| `wm`, `w=`, `wR` | Fill View with Window, Make Windows Equal, Rotate Windows Back | Maximize Window, Equalize Windows, Rotate Windows Backward |
| `wH wJ wK wL` | Move Window to Far Left, to Bottom, to Top, to Far Right | Move Window to Left, Bottom, Top, Right Edge |
| `wg`, `wa` | Move Window…, Add Tab to Split… | the same with three dots |
| `v <leader>ae as at ad` | Explain, Summarize, Translate, Define the Selection | … Selection |
| `v <leader>af`, `aa` | Fact-Check the Selection, Ask About the Selection… | Fact Check Selection, Ask About Selection |
| `e j`, `k` | Down, Up | Next Row, Prev Row |
| `e l`, `o`, `h` | Open, or Right on the Foot; Open, Keep Explorer; Close Folder | Open / Expand; Preview; Collapse |
| `e zR`, `zM` | Open All Folders, Close All Folders | Expand All Folders, Collapse All Folders |
| `e s`, `/`, `d` | Jump, Find Tab, Delete | Jump Hints, Filter Rows, Close Tab |
| `e <lt>` | Move out of Folder | Move Out of Folder |
| `e <C-v>`, `<C-s>` | Open in Vertical Split, Open in Horizontal Split | Split Right, Split Below |
| `e R`, `U`, `m`, `+`, `<Tab>` | Reload, Unload, Menu, Create Menu, Select | Reload Tab, Unload Tab, Context Menu, New, Toggle Selection |
| `e q`, `<Esc>` | Close Explorer; Clear Selection or Close | Leave Explorer |
| `t h l 0 $` | Left, Right, First Item, Last Item | Previous Button, Next Button, First Button, Last Button |
| `t <CR> m s i q` | Press, Menu, Jump, Edit Address, Close Toolbar | Activate Button, Button Menu, Jump Hints, Focus Address Bar, Leave Toolbar |

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| WK-01 | P3 | C | The descriptions above differ | Use the fork's text |

## Statusline, mascot and window tint

Fork (`fork: ui/ZendiaStatusline.sys.mjs`, `ZendiaMascot.sys.mjs`, `zendia.css:611-700`):

- Mascot 132 px, 12 from the bottom corner away from the sidebar; typed count and keys beside it with `+group`; it rises
  4 px while typing.
- Mode colors: normal pink, insert green, visual purple, explorer blue, toolbar yellow, library teal, window orange.
  The window and sidebar take the mode color through the space gradient, over 150 ms.
- Full size for 2 s after a change, while keys are pending and while the pointer is within 64 px. Then by setting:
  `shrink` to 40% (default), `always` (shrunk under 700 px wide), `change` (gone).
- Hidden while the page scrolls (back 500 ms after), in DOM fullscreen, with vim off and during onboarding.
- Steps aside from the pointer; keeps 12 px clear of panels, picker and which-key.
- Springs: head 560/20, cells 500/18, letter 600/21, whole 300/31, sizes 320/21; cells pop one after another from the
  mode's cell; blink every 2.6 to 6 s for 180 ms. Reduced motion stops the movement.
- Shows the mode of the web chat page while it has the focus.

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| SL-01 | P1 | C | Not hidden in fullscreen: `set_fullscreen` is never called | Wire it |
| SL-02 | P2 | C | Hides only for mouse wheel scrolling, not `j k` or page scrolling | Report page scrolls |
| SL-03 | P2 | C | No mode for panel pages | With RT-02 |
| SL-04 | P3 | C | The motion is a few shared animations, and the tint a fixed color mix, not the fork's springs and gradient | Compare value by value |
| SL-05 | P3 | R | The port has a visible announcement pill. The fork only tells screen readers about a mode change | Remove it if anything shows it |

## Menus, panels and popups of the browser

Fork (`fork: ui/ZendiaMenuKeys.sys.mjs`): while any menu or keyboard panel is open (main menu, context menus of the
page, tabs and toolbar buttons, toolbar drop-downs, the site panel), it owns the keys and vim keys work in it:

| Keys | Do |
| --- | --- |
| `j`, `<C-n>`, `<C-j>` | down |
| `k`, `<C-p>`, `<C-k>` | up |
| `h`, `l` | left, right (leave and open a submenu) |
| `q`, `<Esc>` | close |

Fields inside a panel keep their keys. A panel that opened on its own (a translation offer) leaves the keys with the
page until the focus goes into it. A permission prompt that does not take the focus leaves the keys with the page.
Extension popups are **not** menus: their keys belong to their page ([Extensions](#extensions)).

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| MN-01 | P1 | C | Menus and panels ignore vim keys; letters jump by type-to-select (`kit/list_nav.rs`). The site panel is moved through with Tab and Enter only | Add the table above to every menu and panel while vim is on |

## Extensions

Extensions are reached from the keyboard like this in the fork. The popup labels are the part the screenshot of uBlock
Origin shows (`a` on the power button, `d f e w o` on the tool icons, `c` and `m` on More and Less).

### Ways in

| Way | What happens |
| --- | --- |
| `<leader>x` (normal, and inherited by explorer, toolbar, library and window modes) | Picker "Extensions" |
| `<leader>m`, `h` `l` to an extension's button, `<CR>` or `o` | Presses it; its popup gets labels |
| A popup opened with the mouse, then `s` | Ordinary [`s` labels](#jump-labels-s-gs-s-and-i) inside the popup |
| uBlock Origin actions (no default key) | See below |
| The extension's own keyboard shortcuts | Work as in Firefox. A vim mapping on the same keys wins |

There are no other extension keys: nothing is mapped under `<leader>x…`.

### The picker

Sources: `fork: plugins/ZendiaExtensionsSpec.sys.mjs`.

- Lists the **running** extensions the user installed (no system or hidden ones) that have a **toolbar action**, sorted
  by name, each with its own icon.
- Picking one presses its toolbar button, exactly as a click does: its popup opens where the button's popups open, or an
  extension without a popup gets the click.
- With none: toast "No extension to run."

### Popup labels

Sources: `fork: core/ZendiaPopupHints.sys.mjs`, `actors/ZendiaPopup*.sys.mjs`, `core/ZendiaKeyClaims.sys.mjs`.

When they appear:

- Only on a popup opened **from the keys** (the picker, the toolbar mode, `ublock.popup`). Opened with the mouse: none.
- The mark is spent on that one opening. A reload inside the same popup keeps the labels. A popup preloaded by hovering
  follows how it is actually opened. A key press that opens nothing (a disabled button, or the button of an open popup,
  which closes it) leaves no mark for a later click; the mark expires after 4 s.

What they are:

- Letters `adfewocmxzbqt`: the ones normal mode leaves free. Up to 13 controls get one letter; with more, all get two
  letters (`aa`, `ad`, …, at most 169; further controls stay unlabelled).
- Every control as `s` counts them, **fields included**, in reading order, in the popup's top document.
- Look: bold 11 on 16 `ui-monospace`, padding 0 4, radius 4, the jump label colors, shadow
  `0 1px 3px rgba(0,0,0,.4)`, at the control's left + 2 and top + 2, kept 24 and 18 from the right and bottom edges. No
  outline, no collision shifting.
- They follow the popup as it changes (DOM changes, resize, and looks at 100, 300 and 800 ms), but not while a label is
  half typed.

Keys while they show:

| Key | Does |
| --- | --- |
| A label | A field takes the focus; anything else gets a mouse move, down and up at its middle. The labels stay |
| First letter of two | Narrows the labels |
| `<Esc>` | With a letter typed: clears it. Otherwise the popup's (it closes) |
| Any other key, or with Ctrl, Alt or Super | Goes to the popup, **where vim keys work** (`j k <C-d> gg G s S v y i`, …) |
| In a focused field | Everything is typed; the labels hide until the focus leaves |

Labels apply only in normal mode.

### uBlock Origin

The fork installs uBlock Origin on first run (the user can turn it down in the onboarding, disable or remove it; once
removed it stays removed). Its own commands are actions with no default key, for user keymaps and agents
(`fork: plugins/ZendiaUblockSpec.sys.mjs`): `ublock.popup` (opens the popup with labels), `ublock.zapper`,
`ublock.picker`, `ublock.logger`, `ublock.dashboard`, `ublock.relax`, `ublock.toggleCosmetic`,
`ublock.toggleJavascript` (the last three ask the user first when an agent runs them). Without uBlock: toast
"uBlock Origin is turned off or not installed."

### Defects

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| EXT-01 | P1 | C | No labels in extension popups, however they are opened | Build the popup labels (after EXT-02) |
| EXT-02 | P0 | C | A popup does not take vim keys, and mapped letters are lost: `surface.rs:313` withholds them using the active **tab's** mode, and the window handler returns while a popup is open (`browser.rs:966`). Typing in a popup's field drops letters; `j k s i` do nothing in it. Part of RT-02 | Give the popup page its own mode, engine and page actions |
| EXT-03 | P1 | R | A popup opened from the keys does not get the keyboard focus: its surface takes focus only on a mouse press, so keys may keep going to the tab behind | Focus the popup when it opens |
| EXT-04 | P2 | C | The popup is anchored at the **last mouse click position** (`browser.rs:551`), not at the extension's button | Anchor it at the button, or the site panel button |
| EXT-05 | P2 | C | The picker lists every enabled extension, with or without a toolbar action, shows version numbers and one generic icon, and says "No extensions installed" (`tab_actions.rs:470`) | Match the fork |
| EXT-06 | P1 | C | uBlock Origin's actions do not exist, and the engine behind the built-in blocker is still a stub (`stub/onboarding.rs:494`) | Add the actions once uBlock runs in the ExtensionServer |
| EXT-07 | P3 | C | An extension shortcut is matched **before** vim, so it wins over a vim mapping (`browser.rs`, `match_extension_shortcut`) | Let vim mappings win |

Pinned extension buttons are part of the port's toolbar mode and run the extension's action.

## Agent, web chat and region keys

### Mappings

All `<leader>a…` mappings of both modes exist in the port ([Appendix A](#appendix-a-every-key-mapping)). Behind them the
port is still a placeholder, as `FreyaUIPort.md` planned: asking sends a fixed prompt naming the page instead of the
selection, image or tabs; `<leader>aR` only says there is no reply; the agent log shows a toast; no agent works a page,
so the fork's rule that **the user's keys keep working while an agent has labels up, and `<Esc>` takes the tab back**
has nothing to attach to yet. Port it with the agents.

### Agents panel (`<leader>aA`)

| Key | Fork (`ui/ZendiaAgentsPanel.sys.mjs`) | Port (`agent/agents_panel.rs`) |
| --- | --- | --- |
| `j`, `<Down>` / `k`, `<Up>` | next / previous | Same |
| `g`, `<Home>` / `G`, `<End>` | first / last | `g` `G` missing |
| `<CR>`, `o` | open the agent's tab and chat | `o` missing |
| `<Space>` | peek on or off | Same |
| `l`, `<Right>` / `h`, `<Left>` | open / close the peek | `h` `l` missing |
| `y` / `n` | allow / deny what the agent asks | Same |
| `a` | new agent | Same |
| `x` | stop | Same |
| `d` | dismiss (not while working) | Same |
| `<Esc>`, `q` | close | `q` missing |

Footer: `j k move · ⏎ open · ␣ peek · x stop · d dismiss · a new`. The list takes the focus when the panel opens.

### Agent panel (the chat box)

| Key | Fork (`ui/ZendiaAgentDock.sys.mjs:1068`) | Port |
| --- | --- | --- |
| `<CR>` (`<S-CR>` newline) | send | Same |
| `<Esc>` | beside the page: back to the page; else close | Same |
| `y` / `n` in an empty box | answer a confirmation | Same |
| `<C-c>` | stop a working agent | Same |
| `<Tab>`, `<S-Tab>` | next, previous installed agent (not while working) | Same |
| `<C-w>` `<C-u>` `<C-h>` | delete word, line, character, undoable; at a line start, join lines | Handled by the text field |
| `<C-v>`, `<S-Insert>` with files copied | attach them | No handling found |
| `<A-Space>` held | dictate, from **anywhere** in the window | Only while the panel is focused |

Nothing typed there reaches the page. The pane keys (`<C-h>` `<C-l>` `<C-\>`) still work, except `<C-h>` in the box.

The dictation key is a preference (`zendia.dictation.hold-key`, default `<A-Space>`). It is taken before vim's keys,
opens the panel if it is closed, and dictates while held.

### Model card

| View | Fork (`ui/ZendiaAgentModelCard.sys.mjs:716`) | Missing in the port (`agent/model_card.rs`) |
| --- | --- | --- |
| All | cancel: `<Esc>`, `<C-c>`, `<C-[>` | `<C-c>`, `<C-[>` |
| Main | effort: `h` `<Left>`, `l` `<Right>`; `0` `<Home>`, `$` `<End>`. `c` context menu. `<CR>` `<Space>` `<Up>` `<Down>` `j` `k` open the model list | `h l 0 $ j k` |
| Model list | `<CR>` pick; `<Down>` `<C-n>`, `<Up>` `<C-p>`; other keys type in the search box | `<C-n>`, `<C-p>` |
| Context menu | `h` `<Left>` back; `<CR>` `<Space>` pick; `j` `<Down>` `<C-n>`, `k` `<Up>` `<C-p>` | `h j k <C-n> <C-p>` |

No key reaches vim or the page while the card is open. The instructions view closes on `<Esc>` and keeps all keys.

### Web chat panel

| Where | Fork (`ui/ZendiaWebChatDock.sys.mjs`) | Port |
| --- | --- | --- |
| The site's page | a vim page: all modes, scroll, jump, insert in its fields | Broken (RT-02) |
| The site's page | `<Esc>` from a field, or unused in normal mode, leaves the panel | Same for closing; modes broken |
| Site strip | `<Left>` `<Right>` change site, wrapping | Same |
| Accounts menu | `<Up>` `<Down>` (wrapping), `<Home>` `<End>`, `<Esc>` close, `<Tab>` close, `<Del>` `<BS>` remove an account | A fixed menu; no remove keys |
| Add a site | `<CR>` adds ("Needs an https address, like chat.mistral.ai"), `<Esc>` cancels | Same |

### Region picker (`<leader>ar`)

Fork (`ui/ZendiaRegionPicker.sys.mjs`): "Drag over what to ask about · Enter takes the whole view · Esc cancels". A drag
of at least 8 by 8 takes the box on release; `<CR>` takes the whole view; `<Esc>`, another mouse button or the window
losing focus cancel; every other key is swallowed. The port's picker matches these, except that `<CR>` during a drag
takes the box drawn so far. What is captured is not handed to anything yet (`stub/pickers.rs:246`).

### When panels and modes meet

Fork (`Zendia.sys.mjs:795-840`): opening the agent panel closes the web chat panel and the reverse. Opening the
explorer, toolbar, library or window mode closes a **floating or sliding** agent or chat panel (one beside the page
stays). The agents panel stays beside the chat it opens unless both would cover the same side. Opening any of these
drops pending keys.

### Defects

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| AGK-01 | P1 | C | Agents panel: `g G o h l q` missing. Keys the panel does not handle may then act on the page (R) | Add them and keep all keys in the list |
| AGK-02 | P1 | C | Model card: vim and Ctrl keys missing (table above) | Add them |
| AGK-03 | P1 | C | Dictation works only with the panel focused and the key cannot be changed | Take it window-wide before vim, open the panel, make it a setting |
| AGK-04 | P2 | R | Pasting copied files or images with `<C-v>` / `<S-Insert>` has no handling in the panel | Attach them |
| AGK-05 | P2 | C | The accounts menu is a fixed list without the remove keys | Fork keys, real accounts |
| AGK-06 | P2 | R | No code found that closes a floating agent or chat panel when a vim mode opens | Match the fork |
| AGK-07 | P1 | T | `<leader>as`: picking an agent only opens or closes the agent panel; the agent is not selected (`build_agents_picker` runs `panel.toggle`) | Select the agent |
| AGK-08 | P1 | T | `<leader>ac`: picking a site fails, because `webchat.open` is not a command (a page menu item names it too) | Add the command, or use the one that selects a site |

## The browser's own pages and Glance

The fork guarantees vim keys everywhere a page is shown, and tests it
(`browser_zendia_internal_pages.js`): modes, `s` and `v` labels and scrolling on Settings, the add-ons manager,
`about:config`, `about:support` and `about:about`; on extension pages in tabs; in extension popups; and inside shadow
roots, **closed ones included**. On the empty new tab, keys work as well.

| ID | Sev | Ev | Defect | Needed |
| --- | --- | --- | --- | --- |
| PG-01 | P1 | C | Settings, with the extension manager in it, opens as a separate native window with no vim keys (its own handler knows Ctrl+F and `<Esc>`, `settings/mod.rs:200`) | Give native surfaces scroll, jump labels, field modes and the statusline, or an approved alternative |
| PG-02 | P1 | C | With Glance open, the active tab is still the parent (`stub/page_layouts.rs`, "Glance does not switch tabs"), so scrolling, jumps and other page keys act on the page **behind** the Glance. The fork selected the Glance's tab | Make page commands target the Glance while it is open |
| PG-03 | P2 | R | New tab, reader, error and other internal pages: confirm that every mode works on them | Check in the running browser |

Closed shadow roots are JMP-05; extension popups are EXT-02.

## Suggested order

| Step | Work | Closes |
| --- | --- | --- |
| 1 | Stop the damage | INS-04, EXP-01, INS-01, RT-01, JMP-01, NAV-01, NAV-20, LIB-01, WIN-01 |
| 2 | Per-page state: mode, pending keys, jump, field focus, for tabs, panel pages and popups | Root causes A, C: MD, RT-02, RT-03, RT-04, EXT-02, EXT-03 |
| 3 | Sidebar and toolbar | EXP, TB, NAV-03, NAV-04, NAV-07, NAV-12 |
| 4 | Library through the engine | LIB |
| 5 | Split layout tree, split pickers, window mode, panel row | WIN |
| 6 | Automation World in frames, with scroll and resize reports | FRM-01, JMP, VIS |
| 7 | CAPTCHA, from the fork's code: math with its tests, recognition, staying jump, cursor | CAP |
| 8 | Extensions, menus, browser pages and Glance | EXT, MN, PG |
| 9 | Agent and web chat keys, yank ring, configuration, statusline | AGK, YNK, CFG, SL |
| 10 | The rest, then text and visual polish | INS, NAV, PIC, WK-01, JMP-06, remaining P3 |

## Rules for whoever ports this

- Follow `Architecture.md` and `AgentGuide.md`: state lives in the core, the window draws it, every action is a command.
- Read the fork's source for the area before changing it. This document summarises; the source decides.
- Work from the fork's code, not from how it looks, so that the results are the same. Where the fork's structure and
  `Architecture.md` differ, follow `Architecture.md` (stated 2026-11-27). Check results by what pages report; use
  screenshots for looks only.
- Test that every command the profile and the pickers name is a known command. Unknown names parse as `Custom` and
  only fail when used (AGK-08).
- Test keys in the state a user presses them in, such as on a tab that is in no split.
- Add a core test for each fixed defect marked T or C that the core can express. The existing tests passed over many of
  these because they checked that a command ran, not what it acted on.
- Upstream seams needed here (focus reporting, frames, mouse drag, scroll reports) go through `FORK.md`.

## Limits of this audit

**This list cannot be proven complete.** Treat it as everything found, and the acceptance checks below as what finds the
rest.

| Check | Done |
| --- | --- |
| Every key of the fork compared with the port's profile | All 409 |
| Every key run against the browser core (`StubBackend`), with a split on screen and without | All the port's keys, except visual text objects |
| Keys with no listed defect traced to the engine call or window code that shows their result | Yes |
| Keys pressed in the real window, with pages that report what they receive | About 60 of the 409 |
| Old and new browser side by side | Jump labels and CAPTCHA keys |

Pressing keys in the real window found INS-04, JMP-07 and VIS-04, which reading the code had missed.

Not tried in the real window: agent and web chat keys, extension keys (the extension service was not built there), the
keys inside the library, most window mode keys, space keys, the yank ring in fields, zoom, and most sidebar and toolbar
keys. Of the CAPTCHA keys, `<leader>ci` `cr` `ca` `cp`, `<C-r>` and `<C-a>` were not tried, and no real CAPTCHA service
was. Not examined in depth: the agent harness and what a web chat question sends (placeholders in the port), the
CAPTCHA watcher (apart from its use by the cursor) and solving agent, the library's and region picker's looks, the
mascot's motion, split view's mouse features, and macOS key handling.

A `—` in Appendix A is not proof that a key works. **An area is done only when each of its keys has been pressed in the
real window**, in more than one state (after switching tabs, in a split, with nothing clicked), and the matching tests of
Appendix B pass.

## Appendix A: every key mapping

The fork maps 409 keys; the port has 407 of them. Only the two Boosts keys are left out, which is
intended. "Port command" is what `Zendia/Base/res/keyboard/vim.toml` runs; all of them are known commands.

"State" lists the defects that concern the key. `—` means no difference was found for that key (it was not tried in the
running browser). Defects that touch **every** key are not repeated: RT-01 to RT-06, MD-01 to MD-03, CFG-03 to CFG-05,
and SL-01 to SL-05. "Placeholder" means the key reaches a command that is not really built yet
([Agent, web chat and region keys](#agent-web-chat-and-region-keys)).

The which-key group names of the fork (spaces, ai, buffer, captcha, file/find, library, quit/session, search, ui,
windows, prev, next, goto, fold, around, inside, ask web chat) must be kept too.

### Normal mode (157 keys)

| Keys | Fork action | Port command | State |
| --- | --- | --- | --- |
| `i` | `input.choose` | `input.choose` | INS-04, INS-01, RT-04, FRM-01 |
| `I` | `mode.insert` | `mode.insert` | MD-01 |
| `gi` | `input.last` | `input.last` | INS-04, INS-02, RT-04, FRM-01 |
| `]i` | `input.next` | `input.next` | INS-04, INS-01, FRM-01 |
| `[i` | `input.prev` | `input.prev` | INS-04, INS-01, FRM-01 |
| `j` | `scroll.down` | `scroll.down` | SL-02, PG-02 |
| `k` | `scroll.up` | `scroll.up` | SL-02, PG-02 |
| `h` | `scroll.left` | `scroll.left` | SL-02, PG-02 |
| `l` | `scroll.right` | `scroll.right` | SL-02, PG-02 |
| `gg` | `scroll.top` | `scroll.top` | SL-02, PG-02 |
| `G` | `scroll.bottom` | `scroll.bottom` | SL-02, PG-02 |
| `<C-d>` | `scroll.halfPageDown` | `scroll.half_page_down` | SL-02, PG-02 |
| `<C-u>` | `scroll.halfPageUp` | `scroll.half_page_up` | SL-02, PG-02 |
| `<C-f>` | `scroll.pageDown` | `scroll.page_down` | SL-02, PG-02 |
| `<C-b>` | `scroll.pageUp` | `scroll.page_up` | SL-02, PG-02 |
| `J` | `tabs.next` | `tab.next` | — |
| `K` | `tabs.prev` | `tab.previous` | — |
| `[b` | `tabs.prev` | `tab.previous` | — |
| `]b` | `tabs.next` | `tab.next` | — |
| `[B` | `tabs.movePrev` | `tab.move_prev` | — |
| `]B` | `tabs.moveNext` | `tab.move_next` | — |
| `<leader>bb` | `tabs.alternate` | `tab.alternate` | — |
| `` <leader>` `` | `tabs.alternate` | `tab.alternate` | — |
| `<leader>bd` | `tabs.close` | `tab.close` | WIN-15 |
| `<leader>bo` | `tabs.closeOthers` | `tab.close_others` | — |
| `<leader>br` | `tabs.closeRight` | `tab.close_right` | — |
| `<leader>bl` | `tabs.closeLeft` | `tab.close_left` | — |
| `<leader>bp` | `tabs.togglePin` | `tab.toggle_pin` | — |
| `<leader>bP` | `tabs.closeUnpinned` | `tab.close_unpinned` | — |
| `<leader>bj` | `tabs.pick` | `tab.pick` | NAV-04 |
| `<leader>bm` | `tabs.menu` | `tab.menu` | NAV-07 |
| `<leader>be` | `tabs.toggleEssential` | `tab.toggle_essential` | — |
| `<leader>bR` | `tabs.rename` | `tab.start_rename` | — |
| `<leader>bc` | `tabs.duplicate` | `tab.duplicate` | — |
| `<leader>bu` | `tabs.unload` | `tab.unload` | NAV-17 |
| `<leader>bM` | `tabs.toggleMute` | `tab.toggle_mute` | NAV-16 |
| `<leader>bf` | `tabs.moveToFolder` | `tab.move_to_folder` | NAV-03 |
| `u` | `tabs.undoClose` | `tab.reopen_closed` | — |
| `<leader>fn` | `tabs.new` | `address_bar.open new_tab=true` | — |
| `H` | `history.back` | `navigation.back` | NAV-08 |
| `L` | `history.forward` | `navigation.forward` | NAV-08 |
| `<C-o>` | `history.back` | `navigation.back` | NAV-08 |
| `<C-i>` | `history.forward` | `navigation.forward` | NAV-08 |
| `r` | `page.reload` | `navigation.reload bypass_cache=false` | — |
| `R` | `page.hardReload` | `navigation.reload bypass_cache=true` | — |
| `-` | `url.up` | `url.up` | — |
| `yy` | `yank.url` | `address_bar.copy_current_url` | — |
| `p` | `url.paste` | `url.paste` | NAV-01 |
| `P` | `url.pasteNewTab` | `url.paste_new_tab` | NAV-01 |
| `s` | `jump.hints` | `jump.hints` | JMP-01 to JMP-06, FRM-01, MD-03 |
| `gs` | `jump.controls` | `jump.controls` | JMP-07, JMP-01 to JMP-06, FRM-01, MD-03 |
| `gc` | `jump.captcha` | `jump.captcha` | CAP-01 to CAP-09 |
| `<leader>cc` | `jump.captcha` | `jump.captcha` | CAP-01 to CAP-09 |
| `<leader>cv` | `captcha.verify` | `captcha.verify` | CAP-01, CAP-02, CAP-07 |
| `<leader>ci` | `captcha.answer` | `captcha.answer` | CAP-01, CAP-02, CAP-07 |
| `<leader>cr` | `captcha.reload` | `captcha.reload` | CAP-01, CAP-02, CAP-07 |
| `<leader>ca` | `captcha.audio` | `captcha.audio` | CAP-01, CAP-02, CAP-07 |
| `<leader>cp` | `captcha.play` | `captcha.play` | CAP-01, CAP-02, CAP-07 |
| `v` | `visual.enter` | `visual.enter` | VIS-04, MD-01, FRM-01, JMP-06 |
| `S` | `jump.text` | `jump.text` | JMP-07, JMP-01, JMP-04, FRM-01, MD-03 |
| `<leader>p` | `yank.history` | `yank.history` | YNK-01 to YNK-05 |
| `/` | `search.page` | `page.find` | NAV-09 |
| `n` | `search.next` | `search.next` | NAV-09 |
| `N` | `search.prev` | `search.prev` | NAV-09 |
| `<leader><Space>` | `find.anything` | `address_bar.open` | — |
| `<leader>ff` | `find.anything` | `location.focus` | Approved difference |
| `<leader>,` | `find.tabs` | `find.tabs` | NAV-18 |
| `<leader>fb` | `find.tabs` | `find.tabs` | NAV-18 |
| `<leader>fr` | `find.recent` | `find.recent` | NAV-18 |
| `<leader>/` | `search.web` | `search.web` | NAV-18 |
| `<leader><Tab>]` | `spaces.next` | `space.next` | — |
| `<leader><Tab>[` | `spaces.prev` | `space.previous` | — |
| `<leader><Tab><Tab>` | `spaces.new` | `space.new` | NAV-15 |
| `<leader><Tab>f` | `spaces.first` | `space.first` | — |
| `<leader><Tab>l` | `spaces.last` | `space.last` | — |
| `<leader><Tab>d` | `spaces.close` | `space.close` | NAV-13 |
| `<leader><Tab>r` | `spaces.rename` | `space.rename` | NAV-19, NAV-20 |
| `<leader><Tab>i` | `spaces.icon` | `space.icon` | NAV-19 |
| `<leader><Tab>t` | `spaces.theme` | `space.theme` | — |
| `<leader><Tab>u` | `spaces.unload` | `space.unload` | NAV-14 |
| `gt` | `spaces.next` | `space.next` | — |
| `gT` | `spaces.prev` | `space.previous` | — |
| `<C-h>` | `panes.focusLeft` | `split.focus_left` | WIN-06, WIN-08 |
| `<C-j>` | `panes.focusDown` | `split.focus_down` | WIN-06, WIN-08 |
| `<C-k>` | `panes.focusUp` | `split.focus_up` | WIN-06, WIN-08 |
| `<C-l>` | `panes.focusRight` | `split.focus_right` | WIN-06, WIN-08 |
| `<C-\>` | `panes.last` | `panes.last` | WIN-06, WIN-08 |
| `<C-Up>` | `splits.growHeight` | `split.grow_height` | WIN-10, WIN-18 |
| `<C-Down>` | `splits.shrinkHeight` | `split.shrink_height` | WIN-10, WIN-18 |
| `<C-Right>` | `splits.growWidth` | `split.grow_width` | WIN-10, WIN-18 |
| `<C-Left>` | `splits.shrinkWidth` | `split.shrink_width` | WIN-10, WIN-18 |
| `<leader>-` | `splits.horizontal` | `split.set_layout direction=vertical` | WIN-01, WIN-10 |
| `<leader>\|` | `splits.vertical` | `split.set_layout direction=horizontal` | WIN-01, WIN-10 |
| `<leader>wh` | `splits.focusLeft` | `split.focus_left` | WIN-08 |
| `<leader>wj` | `splits.focusDown` | `split.focus_down` | WIN-08 |
| `<leader>wk` | `splits.focusUp` | `split.focus_up` | WIN-08 |
| `<leader>wl` | `splits.focusRight` | `split.focus_right` | WIN-08 |
| `<leader>ww` | `splits.next` | `split.next` | WIN-23 |
| `<leader>wp` | `splits.last` | `split.last` | — |
| `<leader>wf` | `splits.pick` | `split.pick` | WIN-05 |
| `<leader>wt` | `splits.close` | `split.remove_tab` | WIN-17, WIN-19, WIN-20 |
| `<leader>wo` | `splits.only` | `split.unsplit` | WIN-20 |
| `<leader>wm` | `splits.zoom` | `split.zoom` | WIN-09, WIN-11, WIN-14 |
| `<leader>w=` | `splits.equalize` | `split.equalize` | WIN-10 |
| `<leader>wx` | `splits.swap` | `split.swap` | WIN-03, WIN-17 |
| `<leader>wr` | `splits.rotate` | `split.rotate` | WIN-10, WIN-17 |
| `<leader>wR` | `splits.rotateBack` | `split.rotate_back` | WIN-10, WIN-17 |
| `<leader>wH` | `splits.edgeLeft` | `split.edge_left` | WIN-03, WIN-13, WIN-17 |
| `<leader>wJ` | `splits.edgeDown` | `split.edge_down` | WIN-03, WIN-13, WIN-17 |
| `<leader>wK` | `splits.edgeUp` | `split.edge_up` | WIN-03, WIN-13, WIN-17 |
| `<leader>wL` | `splits.edgeRight` | `split.edge_right` | WIN-03, WIN-13, WIN-17 |
| `<leader>w+` | `splits.growHeight` | `split.grow_height` | WIN-10, WIN-18 |
| `<leader>w-` | `splits.shrinkHeight` | `split.shrink_height` | WIN-10, WIN-18 |
| `<leader>w>` | `splits.growWidth` | `split.grow_width` | WIN-10, WIN-18 |
| `<leader>w<lt>` | `splits.shrinkWidth` | `split.shrink_width` | WIN-10, WIN-18 |
| `<leader>wg` | `splits.grab` | `split.grab` | WIN-07, WIN-22 |
| `<leader>wa` | `splits.add` | `split.new_empty` | WIN-02, WIN-16, WIN-20 |
| `<leader>w<Space>` | `window.mode` | `window.mode` | WIN-21 |
| `<leader>oo` | `library.toggle` | `library.toggle` | LIB-02 |
| `<leader>oh` | `library.history` | `library.open section=history` | LIB-07, LIB-09 |
| `<leader>od` | `library.downloads` | `library.open section=downloads` | LIB-07, LIB-09 |
| `<leader>os` | `library.spaces` | `library.open section=spaces` | LIB-07, LIB-09 |
| `<leader>om` | `library.media` | `library.open section=media` | LIB-07, LIB-09 |
| `<leader>ob` | `library.boosts` | none | Excluded |
| `<leader>or` | `library.routing` | `library.open section=routing` | LIB-07, LIB-09 |
| `<leader>aa` | `agent.ask` | `agent.ask` | Placeholder |
| `<leader>ap` | `agent.toggle` | `agent.toggle` | Placeholder, AGK-02 to AGK-04 |
| `<leader>aA` | `agent.list` | `agent.list` | Placeholder, AGK-01 |
| `<leader>as` | `agent.select` | `agent.select` | Placeholder, AGK-07 |
| `<leader>am` | `agent.model` | `agent.model` | Placeholder |
| `<leader>ax` | `agent.stop` | `agent.stop` | Placeholder |
| `<leader>an` | `agent.new` | `agent.new` | Placeholder |
| `<leader>al` | `agent.log` | `agent.log` | Placeholder |
| `<leader>aw` | `webchat.toggle` | `webchat.toggle` | RT-02, AGK-05, AGK-06 |
| `<leader>ac` | `webchat.select` | `webchat.select` | Placeholder, AGK-08 |
| `<leader>aN` | `webchat.newChatAll` | `webchat.new_chat_all` | Placeholder |
| `<leader>ak` | `webchat.ask` | `webchat.ask` | Placeholder |
| `<leader>aS` | `webchat.summarizePage` | `webchat.summarize_page` | Placeholder |
| `<leader>ai` | `webchat.askImage` | `webchat.ask_image` | Placeholder |
| `<leader>ar` | `webchat.askRegion` | `webchat.ask_region` | Placeholder |
| `<leader>aT` | `webchat.askTabs` | `webchat.ask_tabs` | Placeholder |
| `<leader>aR` | `webchat.putReply` | `webchat.put_reply` | Placeholder |
| `<leader>x` | `extensions.pick` | `extensions.pick` | EXT-01, EXT-03 to EXT-05 |
| `<leader>?` | `whichkey.show` | `which_key.show_all` | WK-01 |
| `<leader>e` | `explorer.toggle` | `explorer.toggle` | EXP-05 |
| `<leader>E` | `zen.sidebar.toggle` | `sidebar.toggle_expanded` | NAV-12 |
| `<leader>m` | `toolbar.open` | `toolbar.open` | TB-01 |
| `<leader>uc` | `zen.compactMode.toggle` | `layout.toggle_compact_mode` | — |
| `<leader>uR` | `zen.layout.reset` | `layout.reset` | — |
| `<leader>ub` | `ui.toggleDark` | `theme.toggle_dark` | NAV-11 |
| `<leader>uz` | `ui.readerMode` | `site.reader_mode.toggle` | NAV-06 |
| `<leader>un` | `ui.dismissNotifications` | `notification.dismiss_all` | NAV-11 |
| `<leader>qq` | `session.quit` | `app.quit` | — |
| `<leader>qs` | `session.restore` | `session.restore` | NAV-02 |
| `<leader>qm` | `window.minimize` | `window.minimize` | — |
| `<leader>qM` | `window.toggleMaximize` | `window.toggle_maximize` | — |
| `<leader>qw` | `window.close` | `window.close` | NAV-05 |

### Insert mode (9 keys)

| Keys | Fork action | Port command | State |
| --- | --- | --- | --- |
| `<C-j>` | `input.next` | `input.next` | INS-04, INS-01, FRM-01 |
| `<C-k>` | `input.prev` | `input.prev` | INS-04, INS-01, FRM-01 |
| `<Esc>` | `mode.normal` | `mode.normal` | MD-01 |
| `<C-w>` | `edit.deleteWordBackward` | `edit.delete_word_backward` | — |
| `<C-u>` | `edit.deleteToLineStart` | `edit.delete_to_line_start` | INS-03 |
| `<C-h>` | `edit.deleteCharBackward` | `edit.delete_char_backward` | — |
| `<A-p>` | `yank.putPrev` | `yank.put_prev` | YNK-01, YNK-05 |
| `<A-n>` | `yank.putNext` | `yank.put_next` | YNK-01, YNK-05 |
| `<C-\>` | `panes.last` | `panes.last` | WIN-06, WIN-08 |

### Visual mode (110 keys)

| Keys | Fork action | Port command | State |
| --- | --- | --- | --- |
| `<Esc>` | `visual.exit` | `visual.exit` | MD-01 |
| `v` | `visual.exit` | `visual.exit` | MD-01 |
| `y` | `visual.yank` | `visual.yank` | VIS-02 |
| `o` | `visual.swapEnds` | `visual.swap_ends` | MD-01 |
| `h` | `visual.left` | `visual.left` | VIS-01 |
| `l` | `visual.right` | `visual.right` | VIS-01 |
| `j` | `visual.down` | `visual.down` | VIS-01 |
| `k` | `visual.up` | `visual.up` | VIS-01 |
| `<Left>` | `visual.left` | `visual.left` | VIS-01 |
| `<Right>` | `visual.right` | `visual.right` | VIS-01 |
| `<Down>` | `visual.down` | `visual.down` | VIS-01 |
| `<Up>` | `visual.up` | `visual.up` | VIS-01 |
| `w` | `visual.wordForward` | `visual.word_forward` | VIS-01 |
| `e` | `visual.wordForward` | `visual.word_forward` | VIS-01 |
| `b` | `visual.wordBackward` | `visual.word_backward` | VIS-01 |
| `0` | `visual.lineStart` | `visual.line_start` | VIS-01 |
| `^` | `visual.lineStart` | `visual.line_start` | VIS-01 |
| `$` | `visual.lineEnd` | `visual.line_end` | VIS-01 |
| `}` | `visual.paragraphForward` | `visual.paragraph_forward` | VIS-01 |
| `{` | `visual.paragraphBackward` | `visual.paragraph_backward` | VIS-01 |
| `gg` | `visual.top` | `visual.top` | VIS-01 |
| `G` | `visual.bottom` | `visual.bottom` | VIS-01 |
| `s` | `jump.words` | `jump.words` | VIS-04, JMP-06, FRM-01 |
| `S` | `jump.text` | `jump.text` | JMP-07, JMP-01, JMP-04, FRM-01, MD-03 |
| `<C-d>` | `scroll.halfPageDown` | `scroll.half_page_down` | SL-02, PG-02 |
| `<C-u>` | `scroll.halfPageUp` | `scroll.half_page_up` | SL-02, PG-02 |
| `<C-f>` | `scroll.pageDown` | `scroll.page_down` | SL-02, PG-02 |
| `<C-b>` | `scroll.pageUp` | `scroll.page_up` | SL-02, PG-02 |
| `?` | `whichkey.show` | `which_key.show_all` | WK-01 |
| `a(` `a)` `ab` `a[` `a]` `a{` `a}` `aB` `a<lt>` `a>` `a"` `a'` `` a` `` `aq` `aw` `aW` `ae` `ad` `as` `ap` `at` `ag` `a_` `a.` `a,` `a;` `a:` `a+` `a-` `a=` `a~` `a\|` `a/` `a\` `a&` `a$` `a#` | `visual.object` (around, 37 keys) | `visual.object` | VIS-03 |
| `i(` `i)` `ib` `i[` `i]` `i{` `i}` `iB` `i<lt>` `i>` `i"` `i'` `` i` `` `iq` `iw` `iW` `ie` `id` `is` `ip` `it` `ig` `i_` `i.` `i,` `i;` `i:` `i+` `i-` `i=` `i~` `i\|` `i/` `i\` `i&` `i$` `i#` | `visual.object` (inside, 37 keys) | `visual.object` | VIS-03 |
| `<leader>ak` | `webchat.ask` | `webchat.ask` | Placeholder |
| `<leader>ae` | `webchat.explain` | `webchat.explain` | Placeholder |
| `<leader>as` | `webchat.summarize` | `webchat.summarize` | Placeholder |
| `<leader>at` | `webchat.translate` | `webchat.translate` | Placeholder |
| `<leader>ad` | `webchat.define` | `webchat.define` | Placeholder |
| `<leader>af` | `webchat.factCheck` | `webchat.fact_check` | Placeholder |
| `<leader>aa` | `webchat.askAbout` | `webchat.ask_about` | Placeholder |

### Explorer mode (49 keys)

| Keys | Fork action | Port command | State |
| --- | --- | --- | --- |
| `j` | `explorer.down` | `explorer.down` | EXP-04, EXP-16 |
| `k` | `explorer.up` | `explorer.up` | EXP-04, EXP-16 |
| `<Down>` | `explorer.down` | `explorer.down` | EXP-04, EXP-16 |
| `<Up>` | `explorer.up` | `explorer.up` | EXP-04, EXP-16 |
| `gg` | `explorer.first` | `explorer.first` | EXP-13 |
| `G` | `explorer.last` | `explorer.last` | EXP-13 |
| `<C-d>` | `explorer.halfPageDown` | `explorer.half_page_down` | EXP-15 |
| `<C-u>` | `explorer.halfPageUp` | `explorer.half_page_up` | EXP-15 |
| `<CR>` | `explorer.open` | `explorer.open` | EXP-19 |
| `l` | `explorer.right` | `explorer.right` | EXP-02, EXP-04 |
| `o` | `explorer.preview` | `explorer.preview` | EXP-09 |
| `h` | `explorer.collapse` | `explorer.collapse` | EXP-03, EXP-04 |
| `za` | `explorer.foldToggle` | `explorer.fold_toggle` | EXP-16 |
| `zo` | `explorer.foldOpen` | `explorer.fold_open` | EXP-16 |
| `zc` | `explorer.foldClose` | `explorer.fold_close` | EXP-16 |
| `zR` | `explorer.foldOpenAll` | `folder.expand_all` | — |
| `zM` | `explorer.foldCloseAll` | `folder.collapse_all` | — |
| `Z` | `explorer.foldCloseAll` | `folder.collapse_all` | — |
| `s` | `explorer.jump` | `explorer.jump` | EXP-06, EXP-20 |
| `/` | `explorer.find` | `explorer.find` | EXP-08 |
| `a` | `explorer.add` | `tab.new` | EXP-08 |
| `A` | `explorer.addFolder` | `folder.create` | EXP-11 |
| `d` | `explorer.delete` | `tab.close` | EXP-01 |
| `u` | `tabs.undoClose` | `tab.reopen_closed` | — |
| `r` | `explorer.rename` | `tab.start_rename` | EXP-01, EXP-11 |
| `c` | `explorer.duplicate` | `tab.duplicate` | EXP-01 |
| `y` | `explorer.yank` | `explorer.yank` | EXP-17 |
| `Y` | `explorer.yankMarkdown` | `explorer.yank_markdown` | EXP-17 |
| `p` | `explorer.togglePin` | `tab.toggle_pin` | EXP-01 |
| `e` | `explorer.toggleEssential` | `tab.toggle_essential` | EXP-01 |
| `>` | `explorer.moveToFolder` | `tab.move_to_folder` | EXP-01, EXP-10 |
| `<lt>` | `explorer.moveOutOfFolder` | `explorer.move_out_of_folder` | EXP-17 |
| `<A-j>` | `explorer.moveDown` | `explorer.move_down` | EXP-18 |
| `<A-k>` | `explorer.moveUp` | `explorer.move_up` | EXP-18 |
| `<C-v>` | `explorer.splitVertical` | `explorer.split_vertical` | EXP-18, WIN-12 |
| `<C-s>` | `explorer.splitHorizontal` | `explorer.split_horizontal` | EXP-18, WIN-12 |
| `R` | `explorer.reload` | `navigation.reload` | EXP-01 |
| `U` | `explorer.unload` | `tab.unload` | EXP-01, NAV-17 |
| `M` | `explorer.toggleMute` | `tab.toggle_mute` | EXP-01, NAV-16 |
| `m` | `explorer.menu` | `explorer.menu` | EXP-11, NAV-07 |
| `+` | `explorer.create` | `explorer.create` | EXP-11 |
| `<Tab>` | `explorer.toggleSelect` | `explorer.toggle_select` | — |
| `V` | `explorer.visual` | `explorer.visual` | EXP-14 |
| `gt` | `spaces.next` | `space.next` | — |
| `gT` | `spaces.prev` | `space.previous` | — |
| `?` | `whichkey.show` | `which_key.show_all` | WK-01 |
| `q` | `explorer.quit` | `explorer.quit` | EXP-12 |
| `<Esc>` | `explorer.escape` | `explorer.quit` | — |
| `<leader>e` | `explorer.quit` | `explorer.quit` | EXP-12 |

### Toolbar mode (16 keys)

| Keys | Fork action | Port command | State |
| --- | --- | --- | --- |
| `h` | `toolbar.left` | `toolbar.left` | — |
| `l` | `toolbar.right` | `toolbar.right` | — |
| `<Left>` | `toolbar.left` | `toolbar.left` | — |
| `<Right>` | `toolbar.right` | `toolbar.right` | — |
| `0` | `toolbar.first` | `toolbar.first` | — |
| `^` | `toolbar.first` | `toolbar.first` | — |
| `$` | `toolbar.last` | `toolbar.last` | — |
| `<CR>` | `toolbar.press` | `toolbar.press` | TB-05 |
| `o` | `toolbar.press` | `toolbar.press` | TB-05 |
| `m` | `toolbar.menu` | `toolbar.menu` | TB-03 |
| `s` | `toolbar.jump` | `toolbar.jump` | TB-02 |
| `i` | `toolbar.urlbar` | `location.focus` | TB-04 |
| `?` | `whichkey.show` | `which_key.show_all` | WK-01 |
| `q` | `toolbar.quit` | `toolbar.quit` | — |
| `<Esc>` | `toolbar.quit` | `toolbar.quit` | — |
| `<leader>m` | `toolbar.quit` | `toolbar.quit` | — |

### Library mode (38 keys)

| Keys | Fork action | Port command | State |
| --- | --- | --- | --- |
| `j` | `library.down` | `library.down` | LIB-02, LIB-05, LIB-06 |
| `k` | `library.up` | `library.up` | LIB-02, LIB-05, LIB-06 |
| `h` | `library.left` | `library.left` | LIB-02, LIB-05, LIB-06 |
| `l` | `library.right` | `library.right` | LIB-02, LIB-05, LIB-06 |
| `<Down>` | `library.down` | `library.down` | LIB-02, LIB-05, LIB-06 |
| `<Up>` | `library.up` | `library.up` | LIB-02, LIB-05, LIB-06 |
| `<Left>` | `library.left` | `library.left` | LIB-02, LIB-05, LIB-06 |
| `<Right>` | `library.right` | `library.right` | LIB-02, LIB-05, LIB-06 |
| `gg` | `library.first` | `library.first` | LIB-02 |
| `G` | `library.last` | `library.last` | LIB-02 |
| `<C-d>` | `library.halfPageDown` | `library.half_page_down` | LIB-08 |
| `<C-u>` | `library.halfPageUp` | `library.half_page_up` | LIB-08 |
| `<CR>` | `library.open` | `library.open` | LIB-02, LIB-11 |
| `o` | `library.open` | `library.open` | LIB-02, LIB-11 |
| `m` | `library.menu` | `library.menu` | LIB-03 |
| `d` | `library.delete` | `library.delete` | LIB-02, LIB-04 |
| `y` | `library.yank` | `library.yank` | LIB-02, LIB-04 |
| `s` | `library.jump` | `library.jump` | LIB-03 |
| `/` | `library.search` | `library.search` | LIB-07 |
| `i` | `library.search` | `library.search` | LIB-07 |
| `f` | `library.filter` | `library.filter` | LIB-05 |
| `a` | `library.add` | `library.add` | LIB-01 |
| `r` | `library.renameSpace` | `library.rename_space` | LIB-01 |
| `t` | `library.spaceTheme` | `library.space_theme` | LIB-01 |
| `I` | `library.spaceIcon` | `library.space_icon` | LIB-01 |
| `<A-h>` | `library.moveSpaceLeft` | `library.move_space_left` | LIB-01 |
| `<A-l>` | `library.moveSpaceRight` | `library.move_space_right` | LIB-01 |
| `<Tab>` | `library.nextSection` | `library.next_section` | — |
| `<S-Tab>` | `library.prevSection` | `library.prev_section` | — |
| `gh` | `library.gotoHistory` | `library.goto_history` | LIB-10 |
| `gd` | `library.gotoDownloads` | `library.goto_downloads` | LIB-10 |
| `gs` | `library.gotoSpaces` | `library.goto_spaces` | LIB-10 |
| `gm` | `library.gotoMedia` | `library.goto_media` | LIB-10 |
| `gb` | `library.gotoBoosts` | none | Excluded |
| `gr` | `library.gotoRouting` | `library.goto_routing` | LIB-10 |
| `?` | `whichkey.show` | `which_key.show_all` | WK-01 |
| `q` | `library.quit` | `library.quit` | LIB-02 |
| `<Esc>` | `library.quit` | `library.quit` | LIB-02 |

### Window mode (30 keys)

| Keys | Fork action | Port command | State |
| --- | --- | --- | --- |
| `h` | `splits.focusLeft` | `split.focus_left` | WIN-08 |
| `j` | `splits.focusDown` | `split.focus_down` | WIN-08 |
| `k` | `splits.focusUp` | `split.focus_up` | WIN-08 |
| `l` | `splits.focusRight` | `split.focus_right` | WIN-08 |
| `w` | `splits.next` | `split.next` | WIN-23 |
| `p` | `splits.last` | `split.last` | — |
| `f` | `splits.pick` | `split.pick` | WIN-05 |
| `s` | `splits.horizontal` | `split.set_layout direction=vertical` | WIN-01, WIN-10 |
| `v` | `splits.vertical` | `split.set_layout direction=horizontal` | WIN-01, WIN-10 |
| `t` | `splits.close` | `split.remove_tab` | WIN-17, WIN-19, WIN-20 |
| `o` | `splits.only` | `split.unsplit` | WIN-20 |
| `m` | `splits.zoom` | `split.zoom` | WIN-09, WIN-11, WIN-14 |
| `=` | `splits.equalize` | `split.equalize` | WIN-10 |
| `x` | `splits.swap` | `split.swap` | WIN-03, WIN-17 |
| `r` | `splits.rotate` | `split.rotate` | WIN-10, WIN-17 |
| `R` | `splits.rotateBack` | `split.rotate_back` | WIN-10, WIN-17 |
| `H` | `splits.edgeLeft` | `split.edge_left` | WIN-03, WIN-13, WIN-17 |
| `J` | `splits.edgeDown` | `split.edge_down` | WIN-03, WIN-13, WIN-17 |
| `K` | `splits.edgeUp` | `split.edge_up` | WIN-03, WIN-13, WIN-17 |
| `L` | `splits.edgeRight` | `split.edge_right` | WIN-03, WIN-13, WIN-17 |
| `+` | `splits.growHeight` | `split.grow_height` | WIN-10, WIN-18 |
| `-` | `splits.shrinkHeight` | `split.shrink_height` | WIN-10, WIN-18 |
| `>` | `splits.growWidth` | `split.grow_width` | WIN-10, WIN-18 |
| `<lt>` | `splits.shrinkWidth` | `split.shrink_width` | WIN-10, WIN-18 |
| `g` | `splits.grab` | `split.grab` | WIN-07, WIN-22 |
| `a` | `splits.add` | `split.new_empty` | WIN-02, WIN-16, WIN-20 |
| `<CR>` | `window.drop` | `window.drop` | WIN-03, WIN-07 |
| `<Esc>` | `window.escape` | `window.escape` | WIN-04 |
| `q` | `window.quit` | `window.quit` | — |
| `?` | `whichkey.show` | `which_key.show_all` | WK-01 |

### Actions without a default key

Users can map these in `zendia.json` (CFG-04), and agents run many of them. The fork also turns every other Zen command
into an action named `zen.<name>`.

| Fork action | What it does | Port |
| --- | --- | --- |
| `input.first` | Focus the first field | Missing |
| `jump.land` | Jump to text and put the visual cursor there | `jump.land` |
| `edit.deleteCharForward`, `edit.deleteToLineEnd` | More insert editing | Second one missing; first not found |
| `find.bookmarks` | Search bookmarks in the URL bar | Missing |
| `splits.grid` | Lay the split out as a grid | `split.set_layout` |
| `ui.appMenu` | Open the main menu | `menu.app.toggle` |
| `captcha.fill` | Type a CAPTCHA answer (agents) | Missing |
| `yank.put`, `yank.clear` | Put a yank, empty the history | `yank.put`, `yank.clear` |
| `yank.urlMarkdown` | Copy the address as a Markdown link | Missing outside the sidebar |
| `webchat.newChat`, `webchat.reload` | Panel actions | Same names |
| `webchat.accounts`, `webchat.sync` | Accounts menu, refresh logins | Missing |
| `ublock.*` (8 actions) | uBlock Origin's commands | Missing (EXT-06) |
| `agent.observe`, `agent.act`, `agent.extract`, `agent.fetch`, `agent.version` | Page access for agents | Missing |


## Appendix B: the old fork's tests as acceptance checks

In `~/zendia/src/zen/tests/zendia/`. Each of these 157 names is behaviour the port must reproduce; read the test for
the exact expectation. Tick one when the port has a test or a checked run for it.

### `browser_zendia_keys.js`: Keys, scrolling, tabs, splits, pickers, which-key

- [ ] actor matches web pages
- [ ] window frame follows mode
- [ ] j scrolls an inner pane
- [ ] j gg G scroll
- [ ] insert mode follows focus
- [ ] unmapped keys reach the page
- [ ] shift j k switch tabs
- [ ] tab keys treat a split as one tab
- [ ] page keeps apz from scrolling mapped keys
- [ ] pending keys show in statusline
- [ ] slash opens find in page
- [ ] leader comma lists tabs
- [ ] gi focuses the first input
- [ ] p opens the clipboard url
- [ ] p refuses non web urls
- [ ] autofocus does not trap keys in insert mode
- [ ] split picker offers url and tabs
- [ ] leader wo unsplits
- [ ] split divides only the current pane
- [ ] escape dismisses the picker
- [ ] leader oh opens library history
- [ ] split open url uses an empty pane
- [ ] i and field jumps
- [ ] shift i and i without fields pass keys
- [ ] insert mode editing keys
- [ ] ctrl w outside a field still closes the tab
- [ ] the grid picker glides between tiles
- [ ] the urlbar is thin glass that opens in motion
- [ ] which key fades out
- [ ] which key popup
- [ ] leader question mark shows all keys
- [ ] boot keeps the sidebar hidden on the empty tab
- [ ] explorer close hides sidebar on empty tab
- [ ] split makes the new pane right or below
- [ ] split picker choice is for its own pane
- [ ] split keeps tabs of another split alone
- [ ] split inside pinned panes adds one pane
- [ ] a split that pins nothing says nothing
- [ ] split stops at the limit before asking
- [ ] split open url works inside a split
- [ ] close goes back to the pane used last
- [ ] only keeps the current pane showing
- [ ] close and only drop an empty pane
- [ ] focus goes to the pane beside and back
- [ ] focus in a grid and after unequal resizes
- [ ] removing an earlier split keeps the view right

### `browser_zendia_key_delivery.js`: Key routing

- [ ] tab keys leave the empty tab
- [ ] keys with the focus in the browser ui
- [ ] a sequence started in the browser ui
- [ ] browser ui fields keep their keys
- [ ] unmapped keys leave the focus alone
- [ ] panes are reached in normal mode
- [ ] tabs are reached in normal mode
- [ ] a clicked tab keeps its field
- [ ] a chord ends a jump and runs
- [ ] a click ends a jump

### `browser_zendia_vim_mode.js`: Vim switch, layout, mode indicator

- [ ] switch is a row of the app menu
- [ ] switch turns vim mode off and on
- [ ] onboarding looks like plain zen
- [ ] turning vim on leaves the keys to vim
- [ ] sidebar toggle keeps the single toolbar
- [ ] layout reset returns to the defaults
- [ ] the mode indicator shows the mascot and the mode
- [ ] the mode indicator steps aside from the pointer
- [ ] the mode indicator rests after a change
- [ ] the mode indicator ducks while the page scrolls

### `browser_zendia_jump.js`: Jumps, fields, visual mode, yank

- [ ] jump to a control by text and label
- [ ] hints label everything
- [ ] hints mark the letters already pressed
- [ ] hints label what is on screen in a long page
- [ ] find gives the controls in the window in order
- [ ] i labels the fields when there are several
- [ ] i with a count goes to that field
- [ ] i enters a lone field at once
- [ ] i leaves out the fields under a dialog
- [ ] i enters a lone field something clear lies over
- [ ] s labels controls in frames too
- [ ] visual mode selects and yanks
- [ ] visual mode labels words and paragraphs
- [ ] gg and G select to the top and bottom
- [ ] text objects
- [ ] typed jump to a word starts visual mode
- [ ] cycling the ring in a field
- [ ] copy events join the ring
- [ ] keys typed in a frame act on the page

### `browser_zendia_captcha.js`: CAPTCHA keys

- [ ] a challenge stays open for every pick
- [ ] escape leaves a challenge
- [ ] other pages still end after one pick
- [ ] gc labels only the captcha
- [ ] a slider is dragged with the keys
- [ ] escape lets go of a slider where it began
- [ ] a point of a picture is clicked with the keys
- [ ] a capital label holds a piece and a board takes it
- [ ] a picture is pointed at not clicked in the middle
- [ ] a capital label holds what is not known to drag
- [ ] a grid of images is clicked not held
- [ ] a ruler puts a slider on its line
- [ ] enter verifies from the cursor
- [ ] gc says what it labeled
- [ ] fed tokens drive the challenge
- [ ] or page labels a page whose captcha is not found

### `browser_zendia_explorer.js`: Explorer, toolbar, menus

- [ ] explorer opens on the current tab
- [ ] explorer moves and opens tabs
- [ ] explorer tab operations
- [ ] explorer visual and picked selections
- [ ] explorer jump labels
- [ ] explorer m opens the tab menu
- [ ] explorer folders
- [ ] explorer moves tabs into folders
- [ ] leader be toggles essentials from the page
- [ ] leader tab r renames the space
- [ ] explorer alt j moves a row
- [ ] explorer ctrl v splits with the cursor tab
- [ ] explorer r renames a tab
- [ ] explorer keeps leader keys
- [ ] explorer colors the window blue
- [ ] leader bj picks a tab from the page
- [ ] leader bm opens the menu of the current tab
- [ ] leader m moves through the toolbar
- [ ] toolbar i edits the address
- [ ] main menu with vim keys
- [ ] toolbar m opens a button menu
- [ ] page context menu takes vim keys
- [ ] mode indicator stays clear of the sidebar
- [ ] explorer and toolbar take turns
- [ ] compact mode from the toolbar hides the sidebar
- [ ] compact mode from the toolbar menu hides the sidebar
- [ ] compact sidebar stays between explorer and toolbar
- [ ] toolbar shows the compact top bar
- [ ] explorer keys after clicking compact mode off

### `browser_zendia_library.js`: Library

- [ ] history opens in its search box
- [ ] y and d on a visit
- [ ] sections and spaces
- [ ] a click on the page hands it the keys
- [ ] tab switches sections while searching
- [ ] explorer reaches the sidebar foot
- [ ] explorer plus opens the create menu

### `browser_zendia_internal_pages.js`: Browser pages, shadow roots, extension popups

- [ ] keys work on internal pages
- [ ] keys reach into shadow roots
- [ ] keys work on extension pages
- [ ] keys work in extension popups
- [ ] popup labels follow how it was opened
- [ ] s labels a popup opened with the mouse
- [ ] a popup loaded by hover follows its opening
- [ ] a press that opens nothing leaves no mark

### `browser_zendia_ublock.js`: Extensions and uBlock Origin

- [ ] no keys of its own
- [ ] actions run ublock commands
- [ ] a toast without ublock
- [ ] removed stays removed
- [ ] turned down in the onboarding
- [ ] the popup has labels

### Keyboard cases in the agent and web chat tests

- [ ] other buffers close a floating panel (`browser_zendia_webchat.js`)
- [ ] the toolbars keys press chat (`browser_zendia_chat_button.js`)
- [ ] the explorers keys press chat (`browser_zendia_chat_button.js`)
- [ ] chat works with vim mode off (`browser_zendia_chat_button.js`)
- [ ] insert mode keys in the dock (`browser_zendia_agent.js`)
- [ ] the model list searches and picks (`browser_zendia_agent_card.js`)

### Unit tests of the pure logic

Port the cases of these with the code they cover: `test_key_engine.js`, `test_keymap.js`, `test_keys.js`,
`test_which_key.js`, `test_jump_labels.js`, `test_text_objects.js`, `test_yank_ring.js`, `test_captcha.js`,
`test_actions.js`, `test_plugins.js`, `test_specs.js`.
