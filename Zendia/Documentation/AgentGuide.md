# Guide for coding agents working on Zendia

This guide is for an AI coding agent (Gemini, Claude or any other) that works on Zendia's UI port without a reviewer
watching each step. It says what to read, which rules hold, where Freya bites, and how to prove the work is right
before calling it done. Follow it in full. A phase is not done because it compiles; it is done when every check in
"Verifying" below has passed and you have looked at the screenshots yourself.

## Read first

Read these fully before writing code:

- `Zendia/Documentation/Architecture.md`, especially "Where state lives".
- `Zendia/Documentation/FreyaUIPort.md`: "How the port follows the architecture", the inventory rows for your surfaces,
  "Phases" and "What done means for one surface".
- `Zendia/CONTRIBUTING.md`, `Zendia/README.md`, `Zendia/UI/Freya/README.md`.
- What earlier phases built: `git log --stat -10`, then the files those commits touched. Reuse what exists. Earlier
  phases often registered a command or a model type you need (grep `commands.rs`, `model.rs` and `stub.rs` first).
- The reference shots of the old Firefox-based fork: `~/zendia-reference-shots/INDEX.md` and the PNGs for your phase.
  Look at them as images. They are the target for layout, sizes and colors.
- The old fork's source in `~/zendia/src/zen/` (and `src/zen/zendia/`) to learn behaviour, sizes and timings. Read
  only. Its CSS often holds the exact numbers (durations, radii, paddings); its locale files
  (`~/zendia/locales/en-US/browser/browser/*.ftl`) hold the exact strings.

## Rules

- **Only `Zendia/` changes.** No upstream Ladybird file changes, and `Libraries/HubProtocol/messages.txt` stays as it
  is unless the task says otherwise. `Zendia/Meta/fork-surface.sh` must pass.
- **The core owns state; Freya draws it.** Browser state (tabs, spaces, split groups, toasts, settings, ...) lives in
  `Libraries/BrowserCore`, which must not depend on Freya. Freya keeps only view state: hover, focus, drags in
  progress, text being typed, animation progress, which popup is open.
- **Every action is a command.** Add a typed variant to `Command`, its name, its parser arm and a `CommandDescriptor`
  so keymaps and the palette reach it. Handle it in the stub (`stub.rs`, or a child module like
  `stub/page_layouts.rs`), publish events, and make `BrowserModel::apply_event` apply them, because each view keeps
  its own copy of the model and only events update it.
- **View-level requests** (open a popup in this window) follow the existing core-event pattern, such as
  `AddressBarOpenRequested`. They reach every window; list each new one in your report for the phase 6 routing fix.
- **Non-UI logic goes in BrowserCore with unit tests**: geometry, ranking, ordering rules, state changes.
- **License**: every new file starts with the Zendia header (`Copyright (c) 2026-present, the Zendia developers.` and
  `SPDX-License-Identifier: BSD-2-Clause`). Never copy Zen or Firefox code or CSS (MPL); re-express what you learn as
  our own Rust. Zen's SVG icons in `Zendia/Base/res/icons/zen/` may be used.
- **Shortcuts** go in `Zendia/Base/res/keyboard/standard.toml`. Before binding a key, check it is free there and that
  it does not take a standard browser shortcut (for example Ctrl+Shift+P is the private window; the palette is Ctrl+K).
- **Sample data is shared.** `sample_model()` feeds every gallery state of every phase. Adding sample data there can
  change older states (a sample split group or media session showed up in phase 2's sidebar). Prefer setting up data
  in your own gallery fixtures.
- **Git**: commit only when the user asks. Commit messages follow `CONTRIBUTING.md` (`Zendia/UI/Freya: Add ...`) and
  must pass `Meta/lint-commit.sh <message-file>` from the repo root. End with the user's co-author line if they give
  one.
- **Style**: follow the existing Zendia Rust. Doc comments in plain, direct English. No filler comments, no leftover
  debug prints, no dead helper functions.

## Builds and caches

There are two separate builds. Both are incremental and expensive to recreate, so **never delete either of them, nor
anything inside them**: no `rm -rf Build`, no `rm -rf Zendia/target`, no `cargo clean`, no deleting `Build/caches` or
`Build/vcpkg-release`. Only the user decides to delete build output.

| Directory | What it is | Rebuilt by | Needed when |
| --- | --- | --- | --- |
| `Build/release/` | Upstream Ladybird in C++: the Hub, WebContent and the other engine processes | `Meta/ladybird.py build`, or `Zendia/Meta/zendia.py build`/`run` without `--skip-upstream` | Only when upstream files changed, which in this repo means after `Zendia/Meta/merge-upstream.sh`. Zendia work never needs it. |
| `Build/caches/`, `Build/vcpkg-release/` | Compiler and dependency caches for the build above | Same | Deleting them turns the next upstream build into a full build of every dependency, which takes hours |
| `Zendia/target/release/` | Our Rust workspace, optimized: the `zendia` and `zendia-gallery` binaries you run and screenshot | `cargo build --release ...` in `Zendia/` | After every change under `Zendia/` that you want to run |
| `Zendia/target/debug/` | Our Rust workspace, unoptimized: used by `cargo test` and `cargo clippy` | `cargo test`, `cargo clippy` | Every time you run the checks |

What to run after a change:

- **Changed anything under `Zendia/`** (Rust, TOML, icons): `cargo build --release --bin zendia --bin zendia-gallery`
  in `Zendia/`. Cargo rebuilds only what changed, usually in seconds to a couple of minutes. Run with
  `Zendia/Meta/zendia.py run --skip-upstream` or the binaries in `Zendia/target/release/`.
- **Changed nothing upstream**: always pass `--skip-upstream` to `zendia.py build` and `zendia.py run`. Without it they
  run Ladybird's build first, which is slow even when nothing changed.
- **Merged upstream Ladybird**: run `Meta/ladybird.py build` once, then continue as above.
- **Do not add `--debug` to `zendia.py`** unless asked. Running the debug binaries is slow, and it is a second set of
  artifacts.
- **Never set a new `CARGO_TARGET_DIR`** or build in a copy of the repo with its own `target`; that is another full Rust
  build of 9 GB or more. A git worktree must point `CARGO_TARGET_DIR` at `Zendia/target`.

**Several checkouts sharing one target directory** (git worktrees for parallel work) can hand each other stale
crates: a library built from one checkout has a newer timestamp, so cargo reuses it for another checkout whose source
differs. The symptom is an error such as "no function named `x` found" for a function that is plainly in the source.
After merging work from other checkouts, touch the sources of the crates that changed before building, for example
`touch Zendia/Libraries/*/src/*.rs Zendia/Libraries/*/src/*/*.rs`, then build again. Do not clean the target
directory.

If a change does not show up, the build is not broken: check that you rebuilt the same profile you are running
(release vs debug), that the build printed no error, and that you launched the binary from `Zendia/target/release/`.
Cleaning caches is never the fix.

If the disk fills up (it is nearly full; check with `df -h .`), stop and tell the user how much space is left and which
of your own temporary files you removed. Do not free space by deleting build directories or caches on your own.

## Freya 0.5.4 traps

Check Freya's real API in `~/.cargo/registry/src/*/freya-*-0.5.4/` instead of guessing. These cost the most time:

- **Hooks** (`use_state`, `use_animation*`, `use_hook`, `use_focus`, `try_use_*`, ...) go at the top of `render`,
  unconditionally, in the same order every render. Never in a helper fn, loop, `match` arm or `if`. A row or pane that
  needs hooks is its own component with a `render_key`. Breaking this compiles fine and panics only when state changes,
  which is why the click-through test below exists.
- **Paint order is tree depth, not sibling order.** Each element's layer is its parent's layer + 1, and
  `Layer::Overlay` only jumps relative to the parent. Deep page content therefore paints over chrome floated from near
  the root. Symptoms: an overlay that "does not render" (it is underneath) or page text bleeding through a card. Use the
  fixed levels in `UI/Freya/src/ui/theme/mod.rs`: `CHROME_OVERLAY_LEVEL` for floating chrome and toasts,
  `POPUP_OVERLAY_LEVEL` for menus, popovers, dialogs, the address bar and drag ghosts. Inside one subtree,
  `Layer::Relative(8)` is enough. A ring that must show above a child goes in the parent's padding, not as an overlay
  child.
- **`Size::fill()` on a `Position::new_absolute()` element resolves to zero.** Use `Size::percent(100.0)`.
- **`on_sized` areas are window coordinates.** Pointer `global_location()` is too. Subtract the element's own origin
  when you need local coordinates (the gallery draws the shell inside a stage, not at the window's corner).
- **Changing state during render**: compare with `peek()` and use `set_if_modified`, as existing code does, or you
  loop.
- **Timers**: `spawn(async move { timer(duration).await; ... })`; both are in Freya's prelude.
- **Animations**: `use_animation_with_dependencies(&deps, |conf, deps| ...)` with `OnCreation::Finish` and
  `OnChange::Rerun`; the fork's timings live in `ui/theme/motion.rs`.

## Order of work

1. Plan from the reference shots and the fork's source: list the surfaces, their states, the commands and events.
2. Core first: model types, commands, stub handling, events, `apply_event`, unit tests. Run
   `cargo test -p zendia-browser-core`.
3. Freya surfaces, built from the core and the kit (`ui/kit`). Build after each file; fix errors as they come.
4. Gallery: a section with keyed states covering every state of every surface (see `gallery/sections/mod.rs` and
   `gallery/registry.rs`), each usable in light and dark.
5. Verify, as below. Fix, and verify again.
6. Report.

## Verifying

Do every step. Look at every screenshot you take; do not assume.

### 1. Checks

From `Zendia/`:

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

From the repo root: `python3 Zendia/Meta/check-licenses.py` and `Zendia/Meta/fork-surface.sh`. Then
`git status --short` must show only `Zendia/` paths.

### 2. Headless display

Never open windows on the user's Wayland desktop. Use Xvfb with `WAYLAND_DISPLAY` unset, and take screenshots with
`import -window root`:

```
unset WAYLAND_DISPLAY
Xvfb :95 -screen 0 1600x1000x24 >/dev/null 2>&1 & XP=$!
DISPLAY=:95 ./target/release/zendia-gallery --section <id> --state <id> --theme dark --size 1600x1000 & GP=$!
sleep 3.5; DISPLAY=:95 import -window root shot.png
kill $GP; kill $XP
```

Kill only the PIDs you started. Never `pkill -f` a pattern that appears in your own command line (it kills your shell).
There is no xdotool and no window manager. Input goes through XTEST with python-xlib; a venv with it may exist at
`~/.cache/eas-build-tmp/claude-1000/-home-cstralpt-cgx-translation-research-ladybird/f732de7b-8005-455d-835a-b86872c3be83/scratchpad/venv/bin/python`.
If it is gone, make one with `python -m venv` and `pip install python-xlib`. Set X input focus on the toplevel
window yourself, since there is no window manager to do it. Keys are keysym names: `.` is `period`, `/` is `slash`,
`:` is `colon` with Shift; a raw punctuation character gives keycode 0 and an X error.

Build with `cargo build --release --bin zendia --bin zendia-gallery` in `Zendia/`, as "Builds and caches" describes.
Upstream Ladybird is already built; do not rebuild it, and never delete build output. Delete only your own temporary
files (screenshots, test pages, scripts).

### 3. Every gallery state, both themes

Screenshot every state of your sections in dark and light, and compare each with its reference shot side by side
(`magick montage a.png b.png -tile 2x1 -geometry 800x500+4+4 out.png` keeps it readable). Check:

- Everything the state promises is on screen. Something missing usually means a paint-order or absolute-size problem
  (see the traps above), not a logic problem.
- Nothing bleeds through cards, panels or popups.
- Layout, sizes, radii, spacing and colors match the reference; active, hovered and focused states are visible.
- Text fits: no letter-by-letter wrapping in narrow places (`max_lines(1)` and `Overflow::Clip`).
- Contrast is readable in both themes.

Fix and recapture until they match.

### 4. One process, every state

Start the gallery once and click through every section in the left list and every state chip, twice, then use your
surfaces' controls (drag, hover, press buttons, open and close popups, let timers run out). Then check that the
process is still alive and stderr has no panic. Launching each state separately with `--state` misses hook-order
panics, because they only happen when state changes inside one process.

### 5. The real window, with the Hub

Serve small test pages with `python3 -m http.server <port> --bind 127.0.0.1 --directory <dir>`. Give each page its own
background color, a text field and a few links, so screenshots show which page is where. Run
`LADYBIRD_HUB=<repo>/Build/release/libexec/Hub ./target/release/zendia <urls...>` under Xvfb, drive it with XTEST, and
screenshot after each step. Prove the parts that only the real window has: pages render, input reaches the right page
(type into each one), menus from `view_request_context_menu` carry your entries, keyboard shortcuts work. For
animations, take a burst of screenshots right after the trigger; a single delayed shot misses them.

### 6. Regressions

If you touched shared code (window shell, sidebar, toolbar, menus, theme, kit, sample data), screenshot a few states
from earlier phases' gallery sections and confirm they still look as before.

### 7. When something is wrong and you cannot see why

Add temporary `eprintln!("DEBUG...")` lines to tell a logic problem (the state never changes) from a drawing problem
(the state changes but nothing shows; log the element's area from `on_sized`). Remove every one before finishing and
`grep -rn DEBUG` to be sure.

### 8. Review your own diff

Read `git diff` before reporting. Look for: leftover debug output, unused imports or `#[allow(dead_code)]` helpers,
shortcuts that clash, sample-data changes that alter other phases, commands without descriptors or stub handling,
events without `apply_event` arms, state kept in Freya that belongs to the core.

## When the user asks to see the changes

When the user says "I want to see your changes" or similar, give them commands to run on their own desktop, run from
the repo root, plus what to do in the window to reach each new surface. Do not open the windows yourself; the user
runs them. Make sure the release build is current first (`cargo build --release --bin zendia --bin zendia-gallery` in
`Zendia/`), so the commands start quickly.

**The real browser.** `--skip-upstream` skips rebuilding Ladybird when only `Zendia/` changed; the window finds the Hub
in `Build/release` by itself. Any URLs after `--` open as tabs:

```
Zendia/Meta/zendia.py run --skip-upstream -- https://example.org https://ladybird.org
```

**The gallery** shows every surface in every state with sample data and needs no Hub. Name the section to open (its
`id` in `gallery/registry.rs`; the left list marks each section's phase, such as P4):

```
Zendia/Meta/zendia.py gallery
Zendia/Meta/zendia.py gallery -- --section <section-id> --state <state-id> --theme light
```

`--theme` takes `light`, `dark` or `sys`; `--size 1600x1000` sets the window size.

Then list, per surface, how to reach it, with the exact keys, menu entries and palette queries. For example, from
phase 4:

- Split view: right-click a tab in the sidebar and choose **Add Split View...**; drag the bar between panes to resize;
  Ctrl+Alt+G, V and H change the layout, Ctrl+Alt+U unsplits.
- Glance: right-click a link on a page and choose **Open Link in Glance**; Escape closes it.
- Compact mode: Ctrl+S; move the pointer to the window's sidebar edge; Ctrl+Alt+S keeps the sidebar out.
- Debug-only data: Ctrl+K opens the palette; type "sample download" or "sample media" and press Enter.

Say which parts run on sample data rather than real engine data, so the user does not mistake a stub for a bug.

## Reporting

End with a concise report: files added and changed, what each part does, the commands and events added (marking the
view-level ones), deviations from the plan and why, anything not done or not verified, and the result of every check
above. Say plainly what you did not test. A report that claims a check passed must mean you ran it and read its output.
