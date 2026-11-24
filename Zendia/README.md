# Zendia

Zendia is a web browser built on [Ladybird](https://github.com/LadybirdBrowser/ladybird), with a Rust UI written with
[Freya](https://github.com/freya-rs/freya). It works like a regular browser, with an optional vim mode, plugins,
Chrome and Firefox extensions through [cgx-webext](https://github.com/Planckify-Labs/cgx-webext), and AI agents that
automate tasks in the browser.

This directory holds all of Zendia's own code. The rest of the tree is upstream Ladybird, which we merge weekly and
change only at the few places `FORK.md` lists. The design is in `Documentation/Architecture.md`.

## Layout

| Directory | What it holds |
| --- | --- |
| `Documentation/` | The architecture and other design documents |
| `Protocols/` | The `.ipc` files of our services and of the embedder channel |
| `Libraries/BrowserCore/` | The command registry, browser model, event bus and profile data |
| `Libraries/ModuleApi/` | The contract plugins and extensions use |
| `Libraries/Keymap/` | The keymap engine, profiles and compiled key tables |
| `Libraries/Automation/` | The client side of the Automation World |
| `Libraries/Service/` | The template every service process is built on |
| `Libraries/Sandbox/` | The sandbox profiles of the services |
| `Services/InputServer/` | The keymap engine's process |
| `Services/AgentServer/` | One AI agent session per process |
| `Services/PluginHost/` | One plugin per process |
| `Services/ExtensionServer/` | The cgx-webext core and its adapter |
| `UI/Headless/` | The browser core without a window |
| `Base/res/` | Resources: keyboard profiles, Automation World scripts, example plugins |
| `Tests/` | Tests that span processes |
| `Meta/` | The build wrapper, the upstream merge and the fork check |

The crates are placeholders so far, each describing its job.
The build order is at the end of `Documentation/Architecture.md`.

## Building and running

This directory is a Cargo workspace of its own. Cargo finds it before the engine's workspace, so the engine's
`Cargo.toml` needs no change. Zendia runs upstream's Hub and engine processes, so build those first:

```sh
Zendia/Meta/zendia.py run -- --temporary-profile https://ladybird.org
```

That builds upstream with `Meta/ladybird.py`, then this workspace, then starts the browser. `Zendia/Meta/zendia.py build`
only builds, `--skip-upstream` skips the upstream build, and `Zendia/Meta/zendia.py check` runs the format check,
clippy, the tests and the fork check.

## Merging upstream

```sh
Zendia/Meta/merge-upstream.sh
```

It merges upstream's `master` and then runs `Meta/fork-surface.sh`, which fails if an upstream file we change is missing
from `FORK.md`.
