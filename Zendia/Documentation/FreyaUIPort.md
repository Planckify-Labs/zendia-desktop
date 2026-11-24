# Porting Zendia's UI to Freya

Zendia first shipped as a fork of Zen Browser, which is built on Firefox. That fork lives in `~/zendia`, and its UI is
written in Firefox's chrome JavaScript, XUL and CSS. This plan ports that UI, one part at a time, to the Freya window in
`Zendia/UI/Freya`.

The port covers the UI only. Each surface gets its layout, look, states and motion in Freya. Anything that needs a
service (an agent run, a chat site, sync, the keymap engine) does nothing yet. Each part is wired up later, as the build
order in `Architecture.md` brings in the service behind it.

The port follows `Architecture.md`, and it does not copy the fork's structure. In the fork, each UI module reads and
changes browser state directly. In Zendia, the Freya window is one view of the browser core. It draws what the core
publishes and asks for every change through a command. The section "How the port follows the architecture" below
says what that means for each surface.

The port is fully native on Freya. Only websites, and Ladybird's own internal pages, run on the browser engine. Every
piece of the fork's UI is drawn by Freya in the UI process, including the parts the fork built as web pages, such as
the onboarding. Ladybird's internal pages (`about:settings`, `about:newtab`, `about:history` and the rest) stay as
upstream ships them, so they are not part of the port.

Written on 24 November 2026, against the fork at `2e03e3785` and Freya 0.5.4.

## Scope

**In scope, for every surface:**

- Layout, sizes, spacing, corner radii, glass and shadows, light and dark themes.
- Every visual state: empty, loading, hovered, selected, focused, disabled, error, waiting on the user.
- Motion: slides, fades, springs, the mascot morph, the working ring, streaming text.
- Interactions that stay inside the UI, such as opening and closing panels, resizing, scrolling, switching a panel's
  layout, typing into a box, moving a cursor over a list, and drag previews.
- A way to see every state on demand, with sample data. This is the gallery described below.

**Out of scope for now:**

- The agent harness, the MCP server, CLI detection, model lists, run traces.
- Writing prompts into a chat site's box (the Ask flow) and dictation. Both act on the site's page, so they come with
  the Automation World. The chat sites themselves load for real, since they are ordinary websites.
- CAPTCHA watching and solving, uBlock Origin, sync, session restore, live folder providers, the mods marketplace.
- The keymap engine. Vim keys that only move through a surface's own UI (j/k in a list) are in scope, because they are
  part of how that surface works. Vim modes, counts, macros, the `:` line and page hints in the page are not. They come
  with InputServer and the Automation World.

Every action whose effect lies outside the UI is a command, invoked through `BrowserCore`. During the port, a stub
backend answers those commands with sample data (see "Foundations"). When the real registry and services land, the
backend changes and the UI code stays the same.

## Where the Freya UI stands

`Zendia/UI/Freya/src` (about 1,750 lines) has a Ladybird-style window: a horizontal tab strip, a toolbar with back,
forward, reload and the location bar, and one page view. It can show only one page at a time. `page_view` in
`browser.rs` sends the same `ViewSetViewport` to every tab, so two pages side by side (split view, Glance, the web chat
panel) are not possible yet.

Freya 0.5.4 already has most of the pieces needed: `rect` with `backdrop_blur`, `shadow`, `corner_radius`, `opacity`
and gradient fills; `use_animation` and `use_animation_transition` with easings; `ScrollView` and `VirtualScrollView`;
`Popup`, `Portal`, `Tooltip`, `ContextMenu`, `Menu`, `Select`; `Switch`, `Slider`, `Checkbox`, `Chip`,
`SegmentedButton`; `Input` and `freya-edit`; `Table`; `ResizableContainer` and `drag_drop`; `svg` and `GifViewer`;
`Titlebar`; and theming.

## How the port follows the architecture

### Rules

- **Freya draws, the core owns.** Windows, tabs, spaces, essentials, pinned tabs, folders, split groups, history,
  bookmarks and sessions belong to the browser core (`Architecture.md`, "Where state lives"). The Freya view holds only
  view state: focus, which panels are open, scroll positions, hover, drags in progress, animation progress. A surface
  that needs anything else reads it from the core's model and hears about changes on the event bus. The fork's UI
  modules keep their own copies of browser state (the sidebar is the tab list, for example). The port does not.
- **Everything is a command.** Every button, menu entry, drag that lands, and key in a surface invokes a named, typed
  command (`tab.close`, `space.switch`, `split.add`, `agent.send`, `webchat.ask`). Nothing in `UI/Freya` changes the
  model directly. A command added for a surface is then also available to keymaps, the palette, plugins, extensions and
  agents, as the architecture intends.
- **Built-in UI is a contribution like any other.** Toolbar items, menu entries, palette entries and side panels are
  data in the core, and the Module API uses the same shapes. Chat, Agents and Web chat are built-in toolbar items. An
  extension's button or a plugin's item goes through the same toolbar model, so moving, hiding, hints and the palette
  work for all of them. The fork's per-widget placement code (`ZendiaChatButton`, `ZendiaAgentsButton`) becomes
  placement rules on that one toolbar model.
- **The kit is what declared UI draws with.** Plugins describe simple UI as data, and Freya draws it
  (`Libraries/ModuleApi`). The shared components built in phase 0 are the components that declarative tree names. Any
  component built for a Zendia surface should be generic enough that a plugin could ask for it too.
- **Keys never wait, and Freya has no keymap of its own.** Standard shortcuts resolve in the UI process from the table
  `Libraries/Keymap` compiles. Vim state (the mode, pending keys, counts, which-key's next keys) lives in InputServer and
  reaches the UI as events. The mode indicator, which-key, explorer and toolbar focus, and window mode draw those
  events. They never decide the mode themselves. The current hard-coded shortcuts in `browser.rs` move into a standard
  table in Keymap's format.
- **Hints are drawn by the UI.** The architecture places hint labels in the UI, over the page, so pages cannot hide
  them. Hints on browser UI need only Freya. Hints on page elements draw at rectangles the Automation World reports, so
  the port draws them from sample rectangles until step 5.
- **Only websites run on the engine.** A Hub view shows a website and nothing else: a tab, a split view pane, Glance,
  and the chat sites in the web chat panel. The one exception is Ladybird's own internal pages (`about:settings`,
  `about:newtab`, `about:history`, `about:downloads`, `about:bookmarks`, `about:version` and the others). They load in a
  tab as upstream ships them and are not ported. All of the fork's UI is Freya. What the fork drew as HTML becomes
  native too: the agent's pages become declared UI drawn with the kit,
  as plugin UI is, and nothing renders HTML in the UI process. Everything around a website is native too: the panel
  it sits in, its header and strips, and every menu, including the context menu a right-click on the page opens (the Hub
  asks the UI to show it, as it asks the Qt UI today).
- **One side panel host.** The fork's `ZendiaDockFrame` (float, slide, split, either side, resizable) becomes the host
  for every side panel: the agent panel, the agents panel, web chat, and plugin panels drawn from declared UI. The
  panel's layout, side and width are profile settings in the core. Whether it is open is view state.
- **Confirmations go through the registry.** The architecture enforces confirmations in the command registry, whoever
  invoked the command. The agent panel's confirmation card is that registry prompt drawn inline, so the same component
  serves plugins and extensions.
- **Secrets never reach the view.** The model chip, model card and agent panel show what AgentServer publishes (model
  names, effort levels, run state). API keys stay in AgentServer.
- **Every service-backed surface has a "service down" state.** A crashed service stops its feature and nothing else
  (`Architecture.md`, "Processes"). The agent panel shows the run as stopped with its trace kept, extension buttons show
  extensions as paused, and plugin panels show the plugin as stopped. These are states to port like any other.
- **One core, any number of views.** Nothing a surface needs may exist only in the Freya crate, because the core also
  runs headless. Sample data and command handling live in `BrowserCore`, so the gallery, the window and later
  `UI/Headless` all use them.
- **No upstream changes.** All of the port lives in `Zendia/`. Several pages on screen at once need only
  `ViewSetViewport` per view and per-view input routing, both of which the Hub protocol already has. If a surface turns
  out to need a seam, it is listed in `FORK.md` first.

### Who owns what each surface shows

| Surfaces | State comes from | During the port |
| --- | --- | --- |
| Sidebar, essentials, pinned tabs, folders, spaces, split groups | Browser core model | Stub backend: tabs from the Hub as today, the rest from sample data |
| Address bar results, library history and downloads | Browser core profile data | Sample data |
| Toolbar, menus, palette | Browser core contributions | Built-in contributions registered at startup |
| Mode indicator, which-key, explorer and toolbar focus, window mode | InputServer events | A debug shortcut emits sample events |
| Page hints, region capture, boosts selector | Automation World | Sample rectangles |
| Agent panel, agents panel, model card, instructions, CAPTCHA summary | AgentServer, through the core's events | Sample conversations and runs |
| Extension buttons, site panel's extensions | ExtensionServer contributions | One sample contribution |
| Mods list, plugin panels | PluginHost contributions | One sample plugin |
| Web chat sites | Ordinary websites in Hub views. The list of sites is a profile setting in the core | The real sites (chatgpt.com, gemini.google.com and so on) load, signed in with the browser's own cookies |
| Panel layout, toolbar layout, compact mode, theme | Profile settings in the core | Defaults from the fork |
| Focus, open panels, scroll, drags, animations | The Freya view | The Freya view |

### Surfaces the architecture adds

- **Command palette.** `Architecture.md` names the palette as a caller of the registry. The fork has none: it used
  pickers and which-key. The palette lists every registered command with its key from the active table, built on the
  picker.
- **Service state.** A way to see that a service is restarting or stopped, shown in the place its feature lives (see
  the rules above).

## Inventory

Line counts are from the fork, without tests. "Pages inside" marks a surface that shows web content and therefore
needs the multi-view work in phase 1.

### Theme and shared pieces (Zendia)

| Surface | Fork source | Lines | What to port |
| --- | --- | --- | --- |
| Theme | `zendia/ui/ZendiaTheme.sys.mjs`, `zendia.css` | 305 + 5,226 | Tokyonight palette (moon on dark, darkened day hues on light), roles (`accent`, `mode-n` … `mode-w`, `md-h1` … `md-h6`, `hint`, `which-key-*`, `success`, `warning`, `error`), glass opacity and blur, per-agent colors. The `zendia.theme` override becomes a profile setting |
| Picker | `ZendiaPicker.sys.mjs` | 273 | Floating list or grid of items with icon, label, detail and key labels. Cursor moves with j/k. Fades in and out. Used by tabs, yank, windows, extensions, agents, web chat |
| Hint labels | `ZendiaHints.sys.mjs` | 112 | Equal-length labels drawn over UI elements |
| Markdown | `ZendiaMarkdown.sys.mjs` | 627 | GitHub-flavored blocks and inline styles, heading hues per level, pipe tables, fenced code. Streaming-safe partial last block |
| Toasts | Zendia toasts in `zendia.css`, Zen `ZenToasts.mjs` | 147 | Short notices: copied, not ready, no selection, and so on |

### Window chrome (Zen)

| Surface | Fork source | Lines | What to port |
| --- | --- | --- | --- |
| Window layout | `common/styles/zen-browser-ui.css`, `zen-browser-container.css`, `ZenUIManager.mjs` | ~1,500 | Page as a rounded card inset from the window, space gradient behind it, window controls |
| Toolbar layouts | `ZenVerticalTabsManager.mjs`, `zen-toolbar.css`, `vertical-tabs-topbar.inc.css` | 764 + CSS | Single toolbar (buttons above the sidebar), multiple toolbars (top bar), collapsed toolbar |
| Sidebar frame | `zen-sidebar.css`, `vertical-tabs.css` | ~1,800 | Vertical tab column, top buttons row, bottom row (space switcher, +, library), resize handle, expanded and collapsed widths |
| App menu | `ZenMenubar.mjs`, Zen's app menu | 142 | Menu with the vim mode switch row above Settings |
| Context menus | Ladybird's `UI/Qt/ContextMenus.cpp` (388 lines), not Zen's | — | Drawn with Freya, but kept to Ladybird's own look, entries and order, not Zen's or Firefox's. The page, link, image, media and selected-text menus come from the Hub's `view_request_context_menu`, as in Ladybird's Qt UI. A page's `<select>` popup works the same way (`view_request_select_dropdown`). Zendia's tab, folder and sidebar menus use the same look. Zendia's own entries (Ask, split, Glance) are added inside Ladybird's menus. Ladybird's code is BSD-2-Clause, so it can be followed closely |
| Sidebar notification | `ZenSidebarNotification.mjs` | 183 | Card at the foot of the sidebar |
| About dialog | `about/zen-about.inc.xhtml`, `zen-about.css` | 72 + 255 | Logo, version, update status, links |

### Tabs, folders and spaces (Zen)

| Surface | Fork source | Lines | What to port |
| --- | --- | --- | --- |
| Vertical tabs | `tabs/zen-tabs/vertical-tabs.css`, `ZenPinnedTabManager.mjs` | 1,506 + 1,201 | Tab rows (favicon, title, close, audio, loading), pinned section, separator with "clear", new tab row, drag reorder preview |
| Essentials | same, `ZenEssentialsPromo.mjs` | 70 + 50 | Grid of icon tiles above the space, promo card when empty |
| Tab renamer | `ZenTabRenamer.mjs` | 168 | Inline rename field in a row |
| Folders | `folders/ZenFolders.mjs`, `ZenFolder.mjs`, `zen-folders.css` | 2,434 + 331 + 338 | Folder rows with icon, collapse and expand animation, nested tabs, active tab shown in a collapsed folder |
| Live folders | `live-folders/ZenLiveFoldersUI.sys.mjs` | 258 | Folder variant with a provider badge (RSS, GitHub). Sample items |
| Spaces | `spaces/ZenSpaceManager.mjs`, `ZenSpace.mjs`, `zen-workspaces.css` | 3,737 + 752 + 535 | Space switcher icons, space header (icon, name), per-space gradient, swipe between spaces |
| Space creation | `ZenSpaceCreation.mjs`, `create-workspace-form.css`, `ZenSpaceIcons.mjs` | 456 + 184 + 274 | Form: name, icon picker, container, theme |
| Gradient generator | `ZenGradientGenerator.mjs`, `zen-gradient-generator.css` | 2,516 + 460 | Color dots on a wheel, texture and opacity, presets, light and dark |

### Page area (Zen and Zendia)

| Surface | Fork source | Lines | What to port |
| --- | --- | --- | --- |
| Split view | `split-view/ZenViewSplitter.mjs`, CSS | 3,210 + 665 | Several pages in a grid, resize handles, drop overlay with the side zones, split group row in the sidebar. Pages inside |
| Window mode | `zendia/ui/ZendiaWindowMode.sys.mjs` | 549 | Numbered pane chips, landing labels at each pane side and split edge, accent ring on the active pane |
| Glance | `glance/ZenGlanceManager.mjs`, `zen-glance.css` | 2,161 + 207 | Page previewed in a card over the current page, with close, expand and split buttons, open and close animation. Pages inside |
| Compact mode | `compact-mode/ZenCompactMode.mjs`, CSS | 1,075 + 446 | Sidebar and top bar hidden, sliding out when the pointer reaches the edge |
| Tab switch motion | `zendia/ui/ZendiaTabMotion.sys.mjs` | 167 | Page slides in from below or above, depending on where the tab is in the sidebar. Skipped on key repeat |
| Media controls | `media/ZenMediaController.sys.mjs`, CSS | 830 + 430 | Card at the foot of the sidebar: title, artist, progress, play and pause, mute, picture-in-picture |
| Download animation | `downloads/ZenDownloadAnimation.mjs` | 554 | Arc from the page into the library button |

### Address bar and site panel (Zen)

| Surface | Fork source | Lines | What to port |
| --- | --- | --- | --- |
| Floating address bar | `urlbar/ZenUrlbarController.mjs`, `zen-omnibox.css` | 417 + CSS | Centered floating bar over a dimmed page, as it opens for a new tab |
| Results | `ZenUBActionsProvider.sys.mjs`, `ZenUBGlobalActions.sys.mjs`, `ZenUBSidebarProvider.sys.mjs` | 592 + 382 + 239 | Rows for sites, history, actions ("Open Library", "Toggle compact mode"), switch to tab |
| Site data panel | `ZenSiteDataPanel.sys.mjs` | 983 | Identity button panel: security, permissions, cookies, the site's extensions |

### Library (Zen, with Zendia's keyboard layer)

| Surface | Fork source | Lines | What to port |
| --- | --- | --- | --- |
| Library shell | `library/ZenLibrary.mjs`, `zen-library.css`, `ZenLibraryWidget.sys.mjs` | 1,200 + 1,496 + 759 | Panel that opens from the sidebar, section tabs, search box, open and close animation |
| Sections | `sections/ZenLibrary*Section.mjs` | 1,155 + 818 + 785 + 578 + 490 | Spaces, downloads, history, space routing, media |
| Media preview | `ZenLibraryMediaPreview.mjs` | 581 | Full-size preview with arrow keys |
| Keyboard layer | `zendia/ui/ZendiaLibraryView.sys.mjs` | 923 | Cursor over rows and grid, C-j/C-k from the search box, focus ring |

### Zen extras

| Surface | Fork source | Lines | What to port |
| --- | --- | --- | --- |
| Boosts editor | `boosts/ZenBoostsEditor.mjs`, `zen-boosts.css`, `zen-boost-editor.inc.xhtml` | 1,823 + 892 + 161 | Per-site restyle panel: color dots, brightness, contrast, saturation, fonts, size, zap, CSS editor |
| Boost share and receive | `ZenBoostShareDialog.mjs`, `zen-boosts-share.css` | 545 + 715 | Share card and receive dialog |
| Element selector and zap | `ZenSelectorComponent.sys.mjs`, `ZenZapDissolve.sys.mjs` | 795 + 636 | Element highlight over the page, dissolve effect. Needs the Automation World later |
| Share | `share/ZenShareManager.mjs`, `zen-share-confirm.inc.xhtml` | 872 + 60 | Share confirmation popup |
| Settings | `browser/components/preferences/zen*.inc.xhtml`, `zen-settings.js` | ~230 + JS | Look and feel (toolbar layout, compact mode, window drag), tab management (pinned tab close behavior, essentials), spaces, Glance triggers, mods |
| Mods | `mods/ZenMods.mjs`, `zenMarketplace.inc.xhtml` | 769 | List of installed mods with switches. In Zendia this is the plugin list (see "Open decisions") |

### Vim mode surfaces (Zendia)

These only show with vim mode on, but they are pure UI. The keymap engine that drives them comes with InputServer.
Until then, a debug shortcut cycles the modes.

| Surface | Fork source | Lines | What to port |
| --- | --- | --- | --- |
| Mode indicator and mascot | `ZendiaStatusline.sys.mjs`, `ZendiaMascot.sys.mjs` | 1,078 + 253 | Pill with the mascot and mode name (NORMAL, INSERT, VISUAL, EXPLORER, TOOLBAR, LIBRARY, WINDOW), typed keys, announcements. Jelly morph between modes, rests shrunk to a badge, ducks while scrolling, moves out of the way of Zendia's panels |
| Which-key | `ZendiaWhichKey.sys.mjs` | 279 | which-key.nvim "helix" popup: title, key, description, groups in their color, which-key's sort |
| Explorer focus | `ZendiaExplorer.sys.mjs` | 786 | Cursor and selection rings over sidebar rows (essentials, pinned, folders, tabs, split groups), jump labels |
| Toolbar focus | `ZendiaToolbar.sys.mjs` | 468 | Cursor over toolbar buttons and the address bar |
| Region picker | `ZendiaRegionPicker.sys.mjs` | 136 | Drag a box over the page, Enter for all of it, Esc to cancel |
| Vim mode switch | `ZendiaVimModeSwitch.sys.mjs` | 106 | App menu row with a switch |

### Agent UI (Zendia)

| Surface | Fork source | Lines | What to port |
| --- | --- | --- | --- |
| Dock frame | `ZendiaDockFrame.sys.mjs` | 609 | Shared by the agent and web chat panels. Float (card over the page), slide (against the edge, hides when the pointer leaves, handle brings it back), split (beside the page, which narrows). Either side, drag to widen, falls back to float when the page would get too narrow. Pill and handle when closed |
| Agent panel | `ZendiaAgentDock.sys.mjs` | 2,543 | Header: agent logo and name, layout picker, other chats of this tab, CAPTCHA summary. Conversation: user messages, answers in Markdown, step groups that fold, tool step rows with icons, confirmation cards (allow, deny), media warnings, sent files. Composer: input, attachments (files, pasted images), model chip, dictation state, hints. Working row with clock and rolling word. Empty state with installed agents. Pill on the closed panel |
| Conversation scroll | `ZendiaAgentFollow.sys.mjs` | 375 | Sent request glides to the top, the answer grows into held space, spring follows long answers, jump-to-end button |
| Agent motion | `ZendiaAgentMotion.sys.mjs` | 811 | Working ring (3.9 s run, 900 ms rest, 30 fps), streaming veil, per-glyph rolling word |
| Model card | `ZendiaAgentModelCard.sys.mjs` | 793 | Effort slider with stops, context window, searchable model list, back navigation |
| Instructions page | `ZendiaAgentInstructionsView.sys.mjs` | 483 | List of instruction cards: switch, title, text, one attached file. Add and done buttons, empty state |
| Agent tables and pages | `ZendiaAgentUiView.sys.mjs` | 455 | Tables the agent sends (sort, filter, copy, open row), drawn natively. The fork lets the agent write HTML pages. Here the agent sends declared UI (the Module API's tree), drawn with the kit, so no engine is needed |
| Agents panel | `ZendiaAgentsPanel.sys.mjs` | 1,007 | Every conversation in every window, grouped: waits on the user, working, finished since last look, the rest. Rows: avatar, request, current step, tab favicon, work list progress, time. Inline permission answer, peek of the last steps, stop, dismiss. Same dock layouts. Pill when an agent waits |
| Buttons | `ZendiaChatButton.sys.mjs`, `ZendiaAgentsButton.sys.mjs` | 327 + 191 | Toolbar widgets: Chat, and Agents with a count badge (amber when one waits). Their placement in single, multiple and compact layouts |

### Web chat (Zendia)

| Surface | Fork source | Lines | What to port |
| --- | --- | --- | --- |
| Web chat panel | `ZendiaWebChatDock.sys.mjs` | 1,746 | Side panel with a site strip (ChatGPT, Claude, Gemini, Kimi, and others), add-site field, drop zone, empty and waiting states. Only the site itself is a website. The panel, header, site strip and every menu (site picker, account, layout, add site) are native Freya. Each site is a regular website in its own Hub view, made when first picked and kept alive, so a reply still coming in, a draft and the scroll stay put. Sites share the browser's cookies like any tab, so the fork's cookie copying goes away. The fork's account menu waits on "Open decisions" |
| Ask menu | `ZendiaWebChatMenu.sys.mjs`, `ZendiaWebChatPrompts.sys.mjs` | 561 + 706 | Native, in the Ladybird-style context menu. "Ask ChatGPT" submenu in the page and tab context menus: a row of site logos, prompts that depend on what was right-clicked, submenus |
| Web chat button | `ZendiaWebChatButton.sys.mjs` | 105 | Icon toolbar widget |
| Dictation view | `actors/ZendiaDictationChild.sys.mjs`, `webchat/ZendiaWebChatDictation.sys.mjs` | — | Listening state in the composer. The hidden site page that does the work comes later |

### Onboarding (Zendia)

| Surface | Fork source | Lines | What to port |
| --- | --- | --- | --- |
| Onboarding | `tools/zendia-welcome/src` (React), `welcome/zendia-welcome.css`, `ZenWelcome.mjs` | 3,285 + 1,658 + 436 | Rebuilt in Freya, not embedded. Full-window pages: start, move in (import), vim, agents, shield (uBlock), apps, go. Each has 2D scenes, the mascot, a browser drawing, procedural audio (a Rust audio crate under an allowed license). Choices become commands |

## Foundations (phase 0)

Every later phase builds on these, so they come first. Items 1 to 3 start `BrowserCore`'s public shape early: only its
interface and a stub behind it. The real registry and model still come in build order step 2.

1. **The core's interface, in `Libraries/BrowserCore`.**
   - `model`: the types the UI draws (windows, spaces, essentials, pinned tabs, folders, tabs, split groups, downloads,
     history entries, media sessions, toolbar items, menu and palette entries, side panels, agents, conversations,
     steps, models, chat sites, service states).
   - `commands`: each command's name and typed arguments, and `invoke`. Each surface adds the commands it needs.
   - `events`: what subscribers hear (tab opened, space switched, mode changed, conversation updated, service
     restarted).
   - `contributions`: the toolbar, menu, palette and panel entries, in the shapes `ModuleApi` will use.
   - No Freya types, so `UI/Headless` and the services can use the crate.

2. **A stub backend.** It answers commands and publishes events from sample data, behind the same interface. Tabs
   still come from the Hub, through a thin adapter over what `browser.rs` does today, so the real window keeps working.
   Everything else is sample data. It logs each command it does not handle yet. Build order step 2 replaces it.

3. **A view on the core in `UI/Freya`.** One subscription that turns the core's events into Freya state, and one way
   for components to invoke commands. Components never reach the Hub or the stub directly.

4. **Module layout.** `UI/Freya/src` keeps the Hub plumbing it has (`hub.rs`, `frames.rs`, `input.rs`, `settings.rs`)
   and gains a `ui/` tree:

   ```
   src/ui/
   ├── core_view.rs      the subscription and invoke (item 3)
   ├── theme.rs          palette, roles, light and dark, glass
   ├── kit/              shared components, also what declared plugin UI draws with
   ├── panels/           the side panel host (float, slide, split)
   ├── chrome/           window, sidebar, toolbar, address bar, menus, palette
   ├── page/             page surfaces, split view, Glance, compact mode
   ├── library/
   ├── vim/              mode indicator, which-key, hints, picker, focus rings
   ├── agent/            agent panel, agents panel, model card, instructions
   ├── webchat/
   └── onboarding/
   ```

5. **Theme.** `ZendiaTheme`'s palette and roles as Rust data, resolved for light or dark, plus glass opacity and blur.
   Components take roles, never raw colors. The theme choice and overrides are a profile setting in the core.

6. **Component kit.** Glass panel, pill, icon button, chip, toast, popover anchored to an element (through `Portal`),
   key label (`<C-j>` style), list row with cursor and selection, confirmation card, switch and slider styled to the
   theme, and fade and slide transitions with the fork's timings. Each one is written so that a node of the Module
   API's declarative tree can map to it.

7. **Icons.** An icon set under an allowed license in `Zendia/Base/res/icons/`, loaded with `svg`. A table maps each
   Zen and Zendia icon name to its replacement.

8. **Page surfaces.** Each Hub view gets its own size and position, with input routed to the view under the pointer.
   Then the window can show any number of websites at once (split view, Glance, web chat),
   with an overlay layer above them for panels and hints. Pages not on screen stop getting frames. This uses the Hub
   messages that exist and changes no upstream file.

9. **Keys through Keymap.** The shortcuts hard-coded in `browser.rs` become a standard profile table in
   `Libraries/Keymap`'s format, resolved in the UI process into commands. Vim surfaces listen for InputServer's events,
   which a debug shortcut fakes until InputServer exists.

10. **Gallery.** A `zendia-gallery` binary that runs the stub backend with no Hub and shows every surface in every
    state, with gray rectangles where pages go. UI work then does not wait for a Ladybird build, and every phase checks
    its work there.

11. **Reference shots.** Screenshots of each surface in the Firefox-based Zendia, in both themes. Each phase is
    compared against them.

## Phases

Each phase ends with its surfaces in the gallery and, where it applies, in the real window, and can be merged on its
own. Each phase also adds to `BrowserCore` the model types, commands and events its surfaces need, with stub handling.

| # | Phase | Surfaces | Core additions | Depends on |
| --- | --- | --- | --- | --- |
| 0 | Foundations | Theme, kit, icons, page surfaces, gallery | Interface, stub backend, Hub tab adapter | |
| 1 | Window shell | Window layout, toolbar layouts, sidebar frame, app menu, context menus, about dialog. The horizontal tab strip goes away | Toolbar and menu contributions, layout settings | 0 |
| 2 | Sidebar content | Vertical tabs, pinned, essentials, folders, live folders, tab renamer, space switcher, space header and gradient, sidebar notification | Spaces, folders, essentials, pinned tabs and their commands | 1 |
| 3 | Address bar and palette | Floating bar, results, site data panel, command palette | Palette entries, result providers, site permissions view | 1 |
| 4 | Page layouts | Split view and its sidebar group, Glance, compact mode, tab switch motion, media controls, download animation, toasts | Split groups, Glance, media sessions, downloads | 0, 2 |
| 5 | Vim surfaces | Mode indicator and mascot, which-key, picker, hints, explorer and toolbar focus, window mode, region picker, vim mode switch | Mode and pending-key events, the vim mode setting | 2, 4 |
| 6 | Side panels and agent | Side panel host, Markdown, agent panel, conversation scroll, motion, model card, instructions page, agent tables and pages, confirmation card, Chat button | Side panel contributions, agent sessions, conversations, steps, models, confirmations | 4, 5 |
| 7 | Agents panel | Agents panel, Agents button with badge | Agent roster events | 6 |
| 8 | Web chat | Web chat panel with the real sites loading, site strip, Ask menu, web chat button, dictation state | The chat site list, the `webchat.ask` command (stub until the Automation World) | 0 (page surfaces), 6 |
| 9 | Library | Library shell, sections, media preview, keyboard layer | History, downloads, space routing | 2 |
| 10 | Onboarding | All seven pages, mascot scenes, audio | First-run settings | 5 |
| 11 | Spaces editing | Space creation form, gradient generator, space swipe | Space appearance | 2 |
| 12 | Boosts and share | Boosts editor, share and receive dialogs, share popup. The selector and zap wait for the Automation World | Per-site boosts | 4 |
| 13 | Settings, plugins and extensions | The native settings window: Zen's groups and Zendia's own (vim mode, agents, web chat sites, theme), with a link to `about:settings` for Ladybird's own settings. Plugin list, extension buttons, service states | Settings, module contributions, service state events | 1, 6 |

Phases 1 to 4 give a browser that looks like Zen. Phases 5 to 8 add what makes Zendia different, which is why they come
before the library and the editors. Phases 9 to 13 can run in parallel once phase 4 is in.

Commits use the categories from `CONTRIBUTING.md`, for example `Zendia/UI/Freya: Add the vertical tab sidebar` and
`Zendia/BrowserCore: Add the space model and its commands`.

### What "done" means for one surface

- It matches its reference shots in both themes, at 1x and 2x scale.
- Each of its states can be shown in the gallery, including its "service down" state where it has one.
- Its motion uses the fork's timings.
- It reads browser state only from the core and changes it only through commands. It keeps only view state itself.
- Its commands are registered with names and typed arguments, so the palette and keymaps can reach them.
- It can be reached with the keyboard and has accessibility names, as the current toolbar does.
- `Zendia/Meta/zendia.py check` passes, and no file outside `Zendia/` changed.

## When the services arrive

The UI stays as it is when the services behind it arrive. The stub backend gives way to the real one:

| Build order step | What replaces the stub |
| --- | --- |
| 2. Browser core | The real registry and model. Tabs, spaces, folders, essentials and split groups become real, and so does `invoke` |
| 3. Service template | Service state events, so the "service down" states show for real |
| 4. InputServer | Mode and pending-key events drive the vim surfaces. The UI resolves keys from compiled tables |
| 5. Automation World | Page hints, region capture, the boosts selector and zap |
| 6. Profile data | History, bookmarks, downloads and sessions in the library and the address bar |
| 7. PluginHost and Module API | Plugin contributions, and declared UI drawn with the kit in side panels |
| 8. cgx-webext docking | Extension buttons, menus and sidebars as contributions, and the site panel's extensions |
| 9. AgentServer and MCP | Agent sessions, conversations, models, confirmations and the CAPTCHA summary |

## Open decisions

- **Zen Mods.** Zen's mods are CSS for Firefox's chrome, which has no counterpart in Freya. The recommendation is to
  cover their use (restyling the browser) with the theme overrides and with plugins, and to drop the marketplace.
- **Several accounts per chat site.** The chat sites run as ordinary websites on Ladybird, with the browser's cookies,
  so web chat needs no process of its own. The Ask flow is a browser core command that writes into the site's page
  through the Automation World, as the agent does. The open question is the fork's account menu, which signs one site
  into several accounts at once. That needs a cookie partition per account in the Hub, which is upstream's code. The
  recommendation is one account per site, the browser's own, until Ladybird has partitions to build on.
- **Plugin web panels.** `Architecture.md` ("Plugins") lets richer plugin UI run as a web panel in WebContent. Under
  the rule that only websites run on the engine, plugins get declared UI only, and that section should change to say
  so. The declarative tree then needs enough components for real panels (lists, forms, tables, Markdown).
- **Extension pages.** WebExtension popups, options pages and sidebars are HTML by the extension standards, so they
  cannot be native. Either they stay on the engine, as Ladybird's internal pages do, shown in Hub views at `chrome-extension://` and
  `moz-extension://`, or extensions ship without their own UI. The first keeps extensions working as users expect.
- **Documents that are not websites.** PDFs (Ladybird's pdf.js), `file://` directory listings and the network error
  page load in a tab as upstream ships them, like the internal pages.
- **Fonts.** Which UI font Zendia ships, since Zen uses the system font and the mascot letters are drawn shapes.
