# Zendia architecture

Zendia is a fork of Ladybird with a Rust UI built on [Freya](https://github.com/freya-rs/freya). It works like a regular
browser out of the box, and vim control is something users turn on. Following Ladybird, each major part runs in its own
process: the keymap engine, AI agents, plugins and extensions. They all drive the browser through one command registry
in the UI process, and Ladybird's own processes stay as upstream ships them, so the fork stays easy to merge.

This is the proposed design, written on 24 November 2026. The `Zendia/` layout below exists. The Freya UI
(`Zendia/UI/Freya` and `Zendia/Libraries/HubProtocol`) has working code, and so do the first parts of BrowserCore,
Keymap and ModuleApi, with a stub backend in place of the real browser core. The services are placeholders.
cgx-webext lives in its own repository.

## Design rules

- **Small fork surface.** Our code lives in `Zendia/`. Upstream files change only at a few listed seams, so merging
  Ladybird stays routine.
- **One job per process.** As in Ladybird, anything that runs untrusted input, holds secrets or can hang gets its own
  process. A crash stops that feature, never the browser.
- **Everything is a command.** Every browser action is a named, typed command in one registry. The window, keymaps,
  plugins, extensions and agents all call it the same way.
- **One core, any number of views.** The registry and the browser model live in the UI process's core, where Ladybird
  keeps browser state today. The Freya window is one view of it, and the core also runs with no window for headless
  agents.
- **Keys never wait.** Standard shortcuts are the default and vim is opt-in. Either way, everyday keys resolve without a
  round trip, and no page can take over the browser's own keys.
- **Rust first.** Ladybird is moving fully to Rust. New code is Rust, and C++ appears only as thin glue in parts not yet
  ported.
- **Repos meet at versioned contracts.** cgx-webext ships on its own schedule. Zendia pins a version and both sides test
  against each other.

## Processes

Ladybird's Hub, renderers and helpers run as upstream ships them (see `Documentation/ProcessArchitecture.md`). Our UI
process is the Hub's client, the same role the Qt UI has today. It holds the browser core (command registry, browser
model, profile data) and launches our four services: InputServer, AgentServer, PluginHost and ExtensionServer. Each
service is a Rust process speaking LibIPC, and each locks itself into its sandbox when it starts.

```
 LLM API, MCP clients                                  PluginHost (one per plugin)
        │ HTTPS, MCP                                        │ commands
        │                                                   │
 AgentServer ──commands──► ┌─────────── UI process ───────────┐ ──keys──►  InputServer
 (one per agent session)   │ browser core: commands, model,   │ ◄─tables─  (keymaps, profiles,
                           │ profile data · Freya view ·      │             macros, scripts)
                           │ launches our services            │
                           └──────────────┬───────────────────┘ ◄─host calls─ ExtensionServer
                                          │ HubServer / HubClient          (cgx-webext core)
                                          ▼ (upstream, unchanged)                │
                                         Hub ◄────────── embedder channel ───────┘
                                          │ navigate, input
                                          ▼
                                     WebContent
                         (Automation World, extension worlds)
```

The Compositor, RequestServer, ImageDecoder and media processes are unchanged and left out. The engine-side seams
(isolated worlds and the embedder channel) are the only places our code reaches into upstream processes.

| Process | How many | Can reach | If it crashes |
| --- | --- | --- | --- |
| UI process | One | The Hub; launches our services | The browser closes, as today |
| Hub and engine processes | As upstream | As upstream | As upstream |
| InputServer | One | The UI process | The UI keeps the last keymap table, falling back to standard shortcuts, and restarts it |
| AgentServer | One per agent session | The UI process; the network only to the model API | That run stops and its trace is kept. Tabs stay as they are |
| PluginHost | One per plugin | The UI process, limited to the plugin's granted capabilities | That plugin stops. Other plugins keep running |
| ExtensionServer | One | The UI process for host calls; the Hub through the embedder channel | Extensions pause, then reload when it restarts |

### Keeping keys instant

A keymap engine in another process could make every keystroke wait on IPC, and a key followed quickly by a click could
arrive out of order. To avoid both, InputServer compiles the active profile into a table and sends it to the UI process.
Ordinary keys resolve there from that table and go straight to the browser core as commands. Only keys that need state
or scripts go to InputServer: counts, macros, the `:` command line, profile scripts.

## The command layer

The browser core is built around one registry. Callers never touch tabs or pages directly. Each caller invokes a
command, the core checks permissions and applies it to the browser model, and the event bus tells every subscriber what
changed. Adding a command makes it available to keys, the palette, plugins, extensions and agents at the same time.

```
 Keyboard (InputServer)      ─┐
 Palette, toolbar (Freya)    ─┤                      ┌───────────────┐         ┌────────────────┐
 Plugins (PluginHost)        ─┼─ invoke(name, args) ─► Command registry ├─applies─► Browser model  ├─drives─► Engine (via the Hub)
 Extensions (ExtensionServer)─┤                      └───────────────┘         └───────┬────────┘
 AI agent (AgentServer)      ─┘                                                        │ publishes
        ▲                                                                              ▼
        └──────── subscribe ──── Event bus: tab opened, page loaded, mode changed, download finished
```

Every caller goes through the same entry point, so permissions, confirmation prompts, macro recording and agent traces
are enforced in one place, whichever process asked.

## Features

### Vim mode (optional)

Off by default, so the browser behaves like any other: Ctrl+T, clicks and normal typing. Turning it on switches the
keyboard scheme to vim, and tabs, panels and pages are then driven from the keyboard. It is an in-house engine, not
Neovim. Hint labels are drawn by the UI over the page, so pages cannot hide or break them.

- Process: InputServer, with a compiled table in the UI process.
- Builds on: the command registry and the Automation World.
- Schemes: standard (the default) and vim are both profiles on one keymap engine. Users can bring their own profiles,
  layered on a base profile and importable from other vim-style browsers' configs.

### Plugins

Sandboxed scripts that add commands, keymaps, panels and agent tools. Simple UI is declared as data and drawn natively
by Freya. Richer UI runs as a web panel in WebContent.

- Process: one PluginHost per plugin.
- Builds on: the Module API, isolated worlds and module schemes.
- Trust: capabilities approved at install.

### WebExtensions

Chrome and Firefox extensions through cgx-webext. They run their own way, with the standard extension APIs, manifest
permissions and pages in WebContent. On the browser side, ExtensionServer connects through the same Module API that
plugins use. An extension's button, menu items and shortcuts become ordinary toolbar items, palette entries and keymaps,
so vim hints, the palette and agents work with them without extra code. The browser core never needs to know what a
WebExtension is.

| | Plugins | WebExtensions |
| --- | --- | --- |
| API | Zendia's own | The standard Chrome and Firefox APIs |
| Runs in | PluginHost | ExtensionServer, plus background pages and content scripts in WebContent |
| Permissions | Capabilities approved at install | Manifest permissions |
| Installed from | Zendia plugin packages | Chrome Web Store, addons.mozilla.org, files |
| Engine seams used | Rarely | The network hook and module schemes |

### AI agent

Reads the page through the accessibility tree, screenshots and the Automation World. Acts by calling commands. Asks
before risky actions, and every run is recorded for audit and replay. It also runs headless, with the browser core and
no window.

- Process: one AgentServer per session, also serving MCP so outside agents can drive Zendia.
- Builds on: the command registry and the Automation World.
- Trust: site scope, confirmations, and the user can stop it at any time.

## Shared hooks

These are the browser's extension points. Each one exists once and has several users.

| Hook | What it provides | Used by |
| --- | --- | --- |
| Command registry | Every browser action as a typed command, in the browser core | Keymaps, palette, plugins, extensions, agents |
| Isolated worlds | A privileged script context inside each page | Vim hints, agents, plugin page scripts, extension content scripts |
| Module schemes | Serves a module's files at its own origin | `plugin://` panels, `chrome-extension://`, `moz-extension://` |
| Network hook | Block, redirect and observe requests | Content blocking, extension rules, agent network policy |
| Embedder channel | One generic message path between our processes and the engine | Automation World, extensions, plugin page scripts |
| Module API | Commands, UI contributions and events for add-ons, over IPC | PluginHost, ExtensionServer |

## Where state lives

| State | Owner | Reason |
| --- | --- | --- |
| Windows and tabs | Browser core | Every service reaches it, and it runs without a window |
| History, bookmarks, sessions | Browser core | Upstream keeps these in the UI process too, so the Hub needs no change |
| Cookies, storage, permissions | Hub | Upstream, unchanged |
| View state | Freya view | Focus, open panels, scroll of the tab strip |
| Keyboard mode, registers, macros | InputServer | Belongs to the keymap engine |
| Extension state | ExtensionServer | Owned by the cgx-webext core |
| Plugin data | Each PluginHost, one store per plugin | Plugins cannot read each other's data |
| Agent runs | AgentServer, one trace per run | Audit and replay |
| Keyboard profiles | User config folder | Belongs to the user and can be shared |

## Trust boundaries

| Untrusted input | Runs in | Can reach |
| --- | --- | --- |
| Web pages | WebContent sandbox | Only what the Hub serves over IPC |
| Extensions | ExtensionServer sandbox and extension worlds | The permissions in their manifest |
| Plugins | Their own sandboxed PluginHost | Capabilities the user approved |
| Page text the agent reads | AgentServer, marked as data in the prompt | Nothing. It cannot grant permissions, and risky actions need the user's confirmation |
| Model API keys | AgentServer only | Never sent to the UI, plugins or pages |
| Shared keyboard profiles | InputServer, scripts sandboxed | Keymaps and settings only |

## Sandboxing

A sandbox is an operating-system lock on a process (seccomp and Landlock on Linux, Seatbelt on macOS). It limits which
files, network and system calls the process can use, so if hostile input takes the process over, the attacker is stuck
inside it. The rule: sandbox every process that handles untrusted input or runs someone else's code. Our services are
launched by the UI process and lock themselves in as they start, before they read any untrusted input.

| Process | Sandboxed? | Why | Allowed |
| --- | --- | --- | --- |
| WebContent, RequestServer, ImageDecoder, media | Yes, by upstream | They run and parse the web | As upstream |
| PluginHost | Yes, strictest | Runs third-party code | IPC to the UI process only. Files and network only through capabilities the core grants |
| ExtensionServer | Yes | Unpacks extension packages from the internet and runs on their data | IPC, plus its own data folder. Fetches go through RequestServer |
| AgentServer | Yes | Reads untrusted page text and holds model API keys | IPC, the network only to the model API, and its trace folder |
| InputServer | Yes, light | Runs profile scripts, which may come from other people | IPC and read-only access to the config folder. No network |
| UI process | No | It is the trusted broker. It needs the window system and profile files, and it launches our services | Everything, as upstream's UI |
| Hub | No, as upstream today | Upstream's decision | As upstream |

A sandbox only limits what a process can reach. For the agent, the real risk is doing the wrong thing through valid
commands, for example after a page tricks it. Site scope and user confirmations in the command registry cover that risk.
Both protections are needed.

## Staying mergeable with upstream

Merging Ladybird should take minutes, not days. Conflicts happen only in files that both sides edit, so the design keeps
that set small, generic and written down.

| Area | Owner | Merge cost |
| --- | --- | --- |
| Freya UI, browser core, our four services, our libraries | Us, in `Zendia/` | None. Upstream never edits it |
| Hub and its UI protocol | Upstream, unchanged | Protocol changes show up as compile errors in one generated Rust crate, not as merge conflicts |
| Engine seams | Upstream files, small edits | Low. Each seam is listed, kept generic, and offered upstream where Ladybird would want it |
| Build | Ours | None. `Zendia/` is its own Cargo workspace and our wrapper builds it |
| Qt UI and LibWebView | Upstream | None, once our extension changes there are reverted and Qt is no longer built |

Practices:

- `Zendia/FORK.md` lists every upstream file we change, and why.
- A CI job measures our diff against upstream outside `Zendia/`. Any growth needs a reviewer's approval.
- Merge upstream on a fixed weekly schedule, because small merges are easy merges. Use merges rather than rebases,
  since other people will build on the fork's history.
- Commits that touch upstream files carry a `zendia:` prefix, as `cgx-webext:` commits do today.
- One generic embedder channel carries all our engine traffic. Without it, every feature would add its own messages
  to upstream's `.ipc` files.
- Every seam that upstream accepts is one less file we have to carry.

## Ready for an all-Rust Ladybird

Process boundaries are also migration boundaries. Peers see a process only through its `.ipc` protocol, so upstream can
port the Hub or any engine service from C++ to Rust without our processes noticing. The engine's IPC generator already
emits Rust senders and receivers (`[RustSender]`, `[RustReceiver]`), and the Freya UI already uses the engine's
`libipc_rust`.

| Part | Today | When Ladybird is all Rust |
| --- | --- | --- |
| UI process and browser core | Rust (Freya and our own crates) | Unchanged |
| Our services | Rust from day one | Unchanged |
| Hub | Upstream's, in C++. We only speak its protocol | Upstream's, in Rust. Nothing changes for us |
| IPC protocols | `.ipc` files, generated for both C++ and Rust | The same files, with the C++ output dropped |
| Wire types | Some hand-written in `ipc_types.rs` | The engine's own Rust types. Our copies are deleted as each one lands |
| cgx-webext | Rust core, C ABI for C++ hosts | Linked directly as Rust. The C ABI stays only for other browsers |
| Engine seams | Written in the language of the engine code around them | Ported with the engine. Their contracts don't change |
| Toolchain | One pinned Rust toolchain at the repo root, also used to build cgx-webext | Unchanged |

Working rules: write no new C++ beyond thin glue over a Rust crate. Define contracts as `.ipc` files or Rust types,
never as C++ headers. Use the engine's Rust crates instead of keeping parallel copies.

## Repositories

There are three repositories. Upstream Ladybird flows into the Zendia fork through regular merges. cgx-webext stays
independent of both and owns a typed contract and a conformance kit. The fork owns the adapter, because the fork changes
faster than the contract. No patches flow between the repos.

```
 Zendia fork (merges upstream weekly)          cgx-webext (own releases)
 ┌─────────────────────────────────────┐       ┌──────────────────────────────────┐
 │ Upstream Ladybird + small seams     │◄╌CI╌╌╌│ webext-conformance · test kit    │
 │ UI process, InputServer,            │       │ webext-shim · chrome.*/browser.* │
 │   AgentServer, PluginHost           │       │                                  │
 │ ExtensionServer (hosts the core)    │─links─► webext-core · decides, no I/O    │
 │ webext adapter (thin)               │─deps──► webext-host · typed contract     │
 └─────────────────────────────────────┘       └──────────────────────────────────┘
```

The fork pins a cgx-webext release, and a local override points it at a working checkout when developing both. When
ExtensionServer starts it exchanges protocol versions with the UI process. If they don't match, extensions turn off and
the browser keeps running.

## Project structure

Everything of ours lives under one top-level `Zendia/` directory that mirrors Ladybird's own layout. Upstream never
edits it, and a future upstream directory can never collide with one of ours.

```
zendia/                            the fork repo
├── AK/  Libraries/  Services/  UI/  Meta/  Tests/  Base/  …
│                                  upstream Ladybird as it is, merged weekly
│
├── Zendia/                        everything ours
│   ├── Cargo.toml                 our Rust workspace and Cargo.lock
│   ├── FORK.md                    ledger of every upstream file we change
│   ├── Documentation/             this document
│   ├── Protocols/                 .ipc files: our services + embedder channel
│   ├── Libraries/
│   │   ├── BrowserCore/           command registry, browser model, events, profile data
│   │   ├── HubProtocol/           client for upstream's Hub
│   │   ├── ModuleApi/             contract for plugins and extensions
│   │   ├── Keymap/                keymap engine, profiles, compiled tables
│   │   ├── Automation/            Automation World client: element refs, snapshots
│   │   ├── Service/               service template: IPC, launch, restart
│   │   └── Sandbox/               per-service sandbox profiles
│   ├── Services/
│   │   ├── InputServer/
│   │   ├── AgentServer/
│   │   ├── PluginHost/
│   │   └── ExtensionServer/       hosts cgx-webext core + adapter
│   ├── UI/
│   │   ├── Freya/                 the window (the zendia executable)
│   │   └── Headless/              browser core without a window
│   ├── Base/res/
│   │   ├── keyboard/              standard.toml (default), vim.toml
│   │   ├── automation-world/      page-side scripts
│   │   └── plugins/examples/
│   ├── Tests/                     services, keymaps, cgx-webext conformance runner
│   └── Meta/                      build wrapper, merge-upstream, fork-surface check
│
└── .github/workflows/zendia-*.yml our CI, beside upstream's untouched workflows

cgx-webext/                        separate repo, own releases
├── crates/
│   ├── webext-core/               today crates/webext
│   ├── webext-host/               new: typed contract
│   ├── webext-shim/               today Shim/
│   ├── webext-conformance/        new: test kit
│   ├── webext-ffi/                C ABI, for C++ browsers
│   └── webext-cli/
├── Data/api-support.json
├── Tests/Fixtures/
└── docs/                          Integration/ladybird goes once the fork has the adapter
```

Some features have to run inside the engine itself, so a few upstream files get small, generic edits. These are the
seams:

| Seam | Upstream files | Why it can't live in `Zendia/` |
| --- | --- | --- |
| Isolated worlds, module schemes | `Libraries/LibWeb/` | Script contexts inside a page and URL schemes like `plugin://` only exist in the engine that runs the page |
| Embedder channel | `Services/Hub/`, `Services/WebContent/` | The only way into a page is through the Hub and WebContent, so one generic message pair is added to their `.ipc` files |
| Network hook | `Services/RequestServer/` | Blocking or redirecting a request has to happen where the request is made |

The build needs no seam. `Zendia/` is its own Cargo workspace, which Cargo finds before the root one, so the root
`Cargo.toml` needs no exclude line. Our build wrapper in `Zendia/Meta/` builds upstream first and then runs Cargo in
`Zendia/`, so the root `CMakeLists.txt` and `Meta/ladybird.py` stay untouched. The `Zendia/` workspace depends on
upstream crates such as `libipc_rust` by path, so we use the engine's code rather than copying it.

## Build order

Each step builds on the one before it. The agent comes last because by then it is mostly a loop over commands and hooks
that the keyboard and plugins have already tested.

1. **Fork setup.** Upstream remote, the weekly merge, `FORK.md` and the fork-surface CI check. Done: see
   `Zendia/Meta/` and `.github/workflows/zendia-ci.yml`. Adding the `upstream` remote waits for Zendia's own repository.
2. **Browser core in the UI process.** Command registry and browser model as Rust crates. The Freya app becomes a view
   on them.
3. **A service template.** Rust LibIPC endpoints, launch from the UI process, a sandbox profile and restart on crash, so
   each new process is cheap to add.
4. **InputServer and keyboard schemes.** Standard shortcuts as the default profile, vim as an opt-in profile, compiled
   tables in the UI process.
5. **Engine seams and the Automation World.** Isolated worlds and the embedder channel, then hints, scrolling the right
   element, caret and visual selection.
6. **Profile data in the browser core.** History, bookmarks and sessions, replacing the Qt UI's stores.
7. **PluginHost and Module API.** One sandboxed process per plugin, declared UI and web panels.
8. **cgx-webext docking.** Typed contract, ExtensionServer and the adapter. Move the extension glue out of LibWebView and
   Qt into ExtensionServer and the seams.
9. **AgentServer and MCP.** Page snapshots, tools from the registry, confirmations and run traces.
10. **Stop building Qt.** Once Freya does everything the Qt UI does. Restore `UI/Qt` to upstream's version instead of
    deleting it, so merges never conflict there.

## Open decisions

- **Which seams to offer upstream.** Isolated worlds and module schemes are the likeliest to be useful to Ladybird
  itself.
- **Plugin language.** Luau is proposed for its built-in sandbox. JavaScript or WASM are the alternatives.
- **Model backend.** A hosted API only, or local models as well.
- **Agent sessions.** Use the person's own profile, or an isolated profile by default.
