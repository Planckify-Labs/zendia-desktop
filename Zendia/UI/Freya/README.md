# Zendia's Freya UI

Zendia's window, written in Rust with [Freya](https://github.com/freya-rs/freya). It replaces the Qt UI in `UI/Qt`.
Until it can do everything the Qt UI does, the two exist side by side.

Like the Qt UI, it is a process of its own. It starts the Hub and drives the views that live there over the Hub's IPC
endpoints (`Services/Hub/HubServer.ipc` and `HubClient.ipc`), through `Zendia/Libraries/HubProtocol`. It links none of
Ladybird's C++ libraries. That also keeps Freya's Skia apart from the C++ Skia the engine uses.

In the planned design (`Zendia/Documentation/Architecture.md`), this becomes a view on the browser core in
`Zendia/Libraries/BrowserCore`, which then holds the tabs, the commands and the profile data.

## Layout

- `src/lib.rs`: shared library crate used by both binaries.
- `src/main.rs`: starts the Hub and opens the `zendia` browser window.
- `src/bin/gallery.rs`: entry point for `zendia-gallery`, the standalone UI component gallery.
- `src/browser.rs`: the window, its tabs and toolbar, and the page view.
- `src/frames.rs`: the backing stores the Compositor presents frames in.
- `src/input.rs`: Freya's keyboard, mouse and wheel events, as Ladybird's.
- `src/settings.rs`: the browser settings the Hub sends.
- `src/ui/`: reactive core view, Tokyonight theme, shared component kit, icons, page surfaces, and keymap wiring.
- `src/gallery/`: extensible section registry and showcase views for `zendia-gallery`.

## UI Gallery

`zendia-gallery` runs against `BrowserCore`'s in-memory stub backend with no Ladybird build and no Hub process. It shows
the theme (light/dark), component kit, icon table, declared UI renderer, page-surface placeholders with the InputServer
overlay, and the standard keymap table:

```sh
Zendia/Meta/zendia.py gallery --debug
# or directly from Zendia/:
cargo run -p zendia --bin zendia-gallery -- --section kit --theme light
```

## Building and running

Build Ladybird as usual first, since the UI runs the Hub and its helper processes from that build:

```sh
./Meta/ladybird.py build
cd Zendia
cargo run --release -p zendia -- --temporary-profile https://ladybird.org
```

Or do both with `Zendia/Meta/zendia.py run -- --temporary-profile https://ladybird.org`.

The UI looks for the Hub in `libexec/` next to its executable or above it, and in `Build/release/libexec/`. Set
`LADYBIRD_HUB` to use another one. Every argument goes to the Hub, as the Qt UI's do, so `--profile=<name>`,
`--temporary-profile` and URLs work the same way.

## What works

- Opening, switching and closing tabs, including tabs that pages open and `window.close()`.
- The location bar, with the search engine and new tab page from the browser's settings.
- Back, forward, reload and stop.
- Pages shown from the frames the Compositor presents in CPU memory, at the display's scale.
- Mouse, wheel and keyboard input to pages, with double and triple clicks.
- The page's cursor, hovered links, and the clipboard text the page reads and writes.
- Shortcuts: Ctrl+T, Ctrl+W, Ctrl+L, Ctrl+R or F5, Alt+Left and Alt+Right.

## Not done yet

In about the order they are planned:

- Context menus, dialogs (`alert()`, `confirm()`, `prompt()`, file pickers, `<select>`), permission prompts and
  downloads.
- Bookmarks, history and sessions. Upstream keeps these stores in the UI process, so they go into the browser core.
- Showing frames without copying them: Wayland subsurfaces or imported dma-bufs on Linux, and IOSurfaces on macOS.
  Each frame is copied out of shared memory now.
- Input methods, through Freya's IME events, and screen readers, through AccessKit and the accessibility tree the Hub
  sends.
- More than one window, private windows, the extensions toolbar, and the platform services the Qt UI provides to the
  Hub (spell checking, translation, location, notifications).
