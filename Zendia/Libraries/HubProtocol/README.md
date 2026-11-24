# HubProtocol

The Rust side of the protocol that upstream's Hub speaks to its UI (`Services/Hub/HubServer.ipc` and `HubClient.ipc`).
We only speak this protocol and never change the Hub, so when upstream changes it, the change shows up here as a build
error rather than as a merge conflict.

- `messages.txt` lists the messages Zendia sends and handles.
- `generate.py` generates them, and the types they carry, from the `.ipc` files of the tree with
  `Meta/Generators/generate_ipc_definitions.py`. A message keeps the ID that the C++ side gives it.
- `src/ipc_types.rs` holds the wire forms of the types that no `.ipc` file declares, each written to match its C++
  `IPC::encode()` and `IPC::decode()`. The generator names any type that is missing from it. These go away as the
  engine's own Rust types land.
- `src/lib.rs` starts the Hub and keeps the connection to it.
- `src/location.rs` turns what is typed in the location bar into a URL.
- `examples/smoke.rs` loads a page in a view without any window and writes the first frame to an image. It checks the
  protocol against the real Hub:

```sh
cd Zendia
cargo run -p hub-protocol --example smoke -- https://ladybird.org frame.ppm
```
