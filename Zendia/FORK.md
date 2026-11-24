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
network hook in `Services/RequestServer/`. The cgx-webext integration below predates the design. Each group says what
it turns into.

## Current changes

### Isolated worlds (seam)

Script contexts for extension content scripts, with realm-scoped CSP, Trusted Types, error reporting and timers. To
become the generic isolated-worlds seam that the Automation World and plugins also use.

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

Extension origins, CORS-free extension fetches, and the hook for declarativeNetRequest, webRequest and webNavigation. To
become the generic module-schemes and network-hook seams.

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

### Extension messages in the Hub and WebContent

The extension_* IPC messages and their handlers. To be replaced by the one generic embedder channel.

- `Services/Hub/Application.cpp`
- `Services/Hub/Application.h`
- `Services/Hub/ConnectionFromUI.cpp`
- `Services/Hub/ConnectionFromUI.h`
- `Services/Hub/HubClient.ipc`
- `Services/Hub/HubServer.ipc`
- `Services/WebContent/ConnectionFromClient.cpp`
- `Services/WebContent/ConnectionFromClient.h`
- `Services/WebContent/PageClient.cpp`
- `Services/WebContent/PageClient.h`
- `Services/WebContent/WebContentClient.ipc`
- `Services/WebContent/WebContentServer.ipc`

### cgx-webext host in LibWebView

The C++ adapter over cgx-webext's C ABI, the store installer and the extension delegate. Moves into ExtensionServer
(build order step 8), then these files go back to upstream's version.

- `Libraries/LibWebView/Application.cpp`
- `Libraries/LibWebView/Application.h`
- `Libraries/LibWebView/CMakeLists.txt`
- `Libraries/LibWebView/Extensions/ApplicationExtensionDelegate.cpp`
- `Libraries/LibWebView/Extensions/ApplicationExtensionDelegate.h`
- `Libraries/LibWebView/Extensions/ExtensionManager.cpp`
- `Libraries/LibWebView/Extensions/ExtensionManager.h`
- `Libraries/LibWebView/Extensions/StoreInstaller.cpp`
- `Libraries/LibWebView/Extensions/StoreInstaller.h`
- `Libraries/LibWebView/Options.h`
- `Libraries/LibWebView/ViewImplementation.cpp`
- `Libraries/LibWebView/WebContentClient.cpp`
- `Libraries/LibWebView/WebContentClient.h`
- `Libraries/LibWebView/WebContentPage.cpp`
- `Libraries/LibWebView/WebContentPage.h`

### Extension UI in Qt

The extensions toolbar, panel and popups of the Qt UI. Reverted to upstream's version once Qt is no longer built (build
order step 10).

- `UI/Qt/Actions.h`
- `UI/Qt/Application.cpp`
- `UI/Qt/Application.h`
- `UI/Qt/CMakeLists.txt`
- `UI/Qt/ExtensionsUI.cpp`
- `UI/Qt/ExtensionsUI.h`
- `UI/Qt/HubConnection.cpp`
- `UI/Qt/HubConnection.h`
- `UI/Qt/RemoteView.h`
- `UI/Qt/WindowToolbar.cpp`
- `UI/Qt/WindowToolbar.h`

### Build option

ENABLE_WEBEXT and WEBEXT_SOURCE_DIR, which build cgx-webext into LibWebView. Removed once ExtensionServer hosts the
core.

- `CMakeLists.txt`
- `Meta/CMake/cmake_options.cmake`

### Web compatibility data

A rule for addons.mozilla.org that names the site, which the combined WebCompat file requires. A candidate to offer
upstream.

- `Base/res/ladybird/WebCompat.json`
