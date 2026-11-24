# Zendia

Zendia is a desktop web browser built on [Ladybird](https://github.com/LadybirdBrowser/ladybird), the independent
browser engine, with a UI written in Rust with [Freya](https://github.com/freya-rs/freya). It works like a regular
browser, and adds:

- an optional vim mode that drives tabs, panels and pages from the keyboard,
- plugins that add commands, keymaps and UI,
- Chrome and Firefox extensions, through [cgx-webext](https://github.com/Planckify-Labs/cgx-webext),
- AI agents that automate tasks in the browser.

Zendia runs on Linux, macOS and Windows. It is at an early stage: the design is written, and most of it is still to
be built.

## How this repository is organized

This repository is a fork of Ladybird. All of Zendia's own code lives in [`Zendia/`](../Zendia). Everything else is
upstream Ladybird, which we merge regularly and change only at the few places
[`Zendia/FORK.md`](../Zendia/FORK.md) lists.

- [`Zendia/README.md`](../Zendia/README.md): the layout, and how to build and run Zendia.
- [`Zendia/Documentation/Architecture.md`](../Zendia/Documentation/Architecture.md): the design.
- [`Zendia/CONTRIBUTING.md`](../Zendia/CONTRIBUTING.md): how to contribute.

The `README.md` at the root of the tree is Ladybird's own, kept as it is so that merges with upstream stay clean.

## Building

Build Ladybird's dependencies as its [build instructions](../Documentation/BuildInstructionsLadybird.md) describe, then:

```sh
Zendia/Meta/zendia.py run -- https://ladybird.org
```

## License

Zendia's own code, in `Zendia/`, is under the BSD 2-Clause license in [`Zendia/LICENSE`](../Zendia/LICENSE). Ladybird
is under the same license, in [`LICENSE`](../LICENSE). Third-party code keeps its own license. Zendia is not
affiliated with or endorsed by the Ladybird project.
