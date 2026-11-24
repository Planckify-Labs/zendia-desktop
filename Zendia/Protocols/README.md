# Protocols

The `.ipc` files of Zendia's own processes, in the same format as upstream's, generated into Rust with upstream's
`Meta/Generators/generate_ipc_definitions.py`:

- `InputServer.ipc`: the UI process and InputServer (keys, compiled key tables, commands).
- `AgentServer.ipc`, `PluginHost.ipc`, `ExtensionServer.ipc`: each service and the UI process's browser core.
- `Embedder.ipc`: the one generic message pair our processes use to reach pages through the Hub and WebContent.

None exist yet. Each is added with the service that speaks it.
