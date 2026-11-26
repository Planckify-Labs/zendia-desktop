# Zendia's changes to upstream files

Zendia is a fork of [Ladybird](https://github.com/LadybirdBrowser/ladybird). Our own code lives in `Zendia/`, which
upstream never edits. This file lists every upstream file we change outside `Zendia/`, and why. Merges with upstream can
only conflict in these files, so the list should stay short.

`Zendia/Meta/fork-surface.sh` checks that every upstream file we change is listed here. CI runs it. Before you change
an upstream file, add it here with the reason. When a change is reverted or accepted upstream, remove it.

Commits that change upstream files carry a `zendia:` prefix in their title. The older `cgx-webext:` commits did the
same.

## Planned seams

The design (`Zendia/Documentation/Architecture.md`) keeps changes to upstream to three generic seams: isolated worlds
and module schemes in `Libraries/LibWeb/`, one embedder channel in `Services/Hub/` and `Services/WebContent/`, and a
network hook in `Services/RequestServer/`.

## Current changes

### Isolated worlds (seam)

Script contexts for the Automation World, plugin page scripts and extension content scripts, with realm-scoped CSP,
Trusted Types, error reporting and timers.

- `Libraries/LibWeb/Bindings/MainThreadVM.cpp`
- `Libraries/LibWeb/Bindings/Wrappable.cpp`
- `Libraries/LibWeb/CMakeLists.txt`
- `Libraries/LibWeb/ContentSecurityPolicy/FetchIntegration.cpp`
- `Libraries/LibWeb/DOM/Document.cpp`
- `Libraries/LibWeb/DOM/Document.h`
- `Libraries/LibWeb/Forward.h`
- `Libraries/LibWeb/HTML/LocalNavigable.cpp`
- `Libraries/LibWeb/HTML/Parser/HTMLParser.cpp`
- `Libraries/LibWeb/HTML/Scripting/Environments.cpp`
- `Libraries/LibWeb/HTML/Scripting/ExceptionReporter.cpp`
- `Libraries/LibWeb/HTML/WindowOrWorkerGlobalScope.cpp`
- `Libraries/LibWeb/Internals/ExtensionBridge.cpp`
- `Libraries/LibWeb/Internals/ExtensionBridge.h`
- `Libraries/LibWeb/Internals/ExtensionBridge.idl`
- `Libraries/LibWeb/Internals/ExtensionWorld.cpp`
- `Libraries/LibWeb/Internals/ExtensionWorld.h`
- `Libraries/LibWeb/Page/Page.h`
- `Libraries/LibWeb/TrustedTypes/TrustedTypePolicy.cpp`
- `Libraries/LibWeb/idl_files.cmake`

### Module schemes and network hook (seams)

Extension origins, CORS-free extension fetches, and the hook for declarativeNetRequest, webRequest and webNavigation.

- `Libraries/LibURL/Rust/src/origin.rs`
- `Libraries/LibWeb/Fetch/Fetching/Fetching.cpp`
- `Libraries/LibWeb/Fetch/Infrastructure/HTTP/Requests.cpp`
- `Libraries/LibWeb/Fetch/Infrastructure/HTTP/Requests.h`
- `Libraries/LibWeb/Loader/ExtensionNetwork.cpp`
- `Libraries/LibWeb/Loader/ExtensionNetwork.h`
- `Libraries/LibWebCommon/Fetch/Infrastructure/URL.h`
- `Libraries/LibWebCommon/Loader/SiteCompatibility.cpp`
- `Libraries/LibWebCommon/Loader/SiteCompatibility.h`
- `Libraries/LibWebCommon/SecureContexts/AbstractOperations.cpp`

### Embedder channel in the Hub, LibWebView and WebContent (seam)

Generic embedder channel (`view_embedder_message` / `view_embedder_reply` for Automation World, and
`connect_embedder_channel` forwarding engine extension hooks to Zendia's sandboxed `ExtensionServer` process). No
`cgx-webext` code or C ABI lives in upstream C++. Two small additions for the embedder's look and vim mode:
`view_set_user_style` / `set_user_style` give every page of a view a user style sheet (Zendia's scrollbar colors),
resent with the other preferences when a view gets a new page, and `PageClient` reports the input method state after
a rendering update that moved the focus, so a field a page focuses by itself (autofocus) reaches the UI too.

- `Libraries/LibWebView/Application.cpp`
- `Libraries/LibWebView/Application.h`
- `Libraries/LibWebView/CMakeLists.txt`
- `Libraries/LibWebView/EmbedderExtensionBridge.cpp`
- `Libraries/LibWebView/EmbedderExtensionBridge.h`
- `Libraries/LibWebView/ViewImplementation.cpp`
- `Libraries/LibWebView/ViewImplementation.h`
- `Libraries/LibWebView/WebContentClient.cpp`
- `Libraries/LibWebView/WebContentClient.h`
- `Libraries/LibWebView/WebContentPage.cpp`
- `Libraries/LibWebView/WebContentPage.h`
- `Services/Hub/ConnectionFromUI.cpp`
- `Services/Hub/ConnectionFromUI.h`
- `Services/Hub/HubClient.ipc`
- `Services/Hub/HubServer.ipc`
- `Services/Hub/View.cpp`
- `Services/WebContent/ConnectionFromClient.cpp`
- `Services/WebContent/ConnectionFromClient.h`
- `Services/WebContent/PageClient.cpp`
- `Services/WebContent/PageClient.h`
- `Services/WebContent/WebContentClient.ipc`
- `Services/WebContent/WebContentServer.ipc`

### Web compatibility data

A rule for addons.mozilla.org that names the site, which the combined WebCompat file requires. A candidate to offer
upstream.

- `Base/res/ladybird/WebCompat.json`
