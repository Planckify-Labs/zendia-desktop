/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! ExtensionServer: Hosts the cgx-webext core and the adapter between its typed contract, the Hub embedder channel,
//! and the UI process's Module API.
//!
//! See `Zendia/Documentation/Architecture.md`, "Processes".

mod server;
mod storage;
mod store;

use std::process::ExitCode;
use zendia_service::{ENV_EMBEDDER_SOCKET_FD, ENV_UI_SOCKET_FD, take_inherited_stream};

fn main() -> ExitCode {
    let ui_stream = match take_inherited_stream(ENV_UI_SOCKET_FD) {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("zendia-extension-server: {error}");
            return ExitCode::FAILURE;
        }
    };
    let hub_stream = match take_inherited_stream(ENV_EMBEDDER_SOCKET_FD) {
        Ok(stream) => Some(stream),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            eprintln!("zendia-extension-server: bad embedder socket: {error}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(error) = server::run(ui_stream, hub_stream) {
        eprintln!("zendia-extension-server: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
