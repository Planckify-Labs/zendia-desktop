# Protocols

The service and embedder-channel protocols of Zendia's own processes:

- `ExtensionServer.md`: versioned protocol (`PROTOCOL_VERSION = 1`) for `ExtensionServer`:
  - UI process <-> `ExtensionServer` over `ZENDIA_UI_FD` (handshake, Module API UI contributions, browser state snapshots, host calls, user prompts, popup requests, install/remove/enable/pin/reload).
  - Hub <-> `ExtensionServer` over `ZENDIA_EMBEDDER_FD`, attached through `HubServer::connect_embedder_channel("extensions", socket)` (serving `chrome-extension://` and `moz-extension://` resources, content script programs, context lifecycle, API calls, network stage hooks, background pages, Hub cookies/browsingData, and RequestServer package downloads).
- Future services (`InputServer`, `AgentServer`, `PluginHost`) share the same framed transport and version handshake from `Zendia/Libraries/Service`.
