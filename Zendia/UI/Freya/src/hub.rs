/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The UI's connection to the Hub, and what the Hub said when the UI started, for every part of the UI to reach.

use crate::settings::Settings;
use hub_protocol::{HubConnection, Incoming, URL_URL};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

pub struct Startup {
    pub connection: Arc<HubConnection>,
    pub incoming: async_channel::Receiver<Incoming>,
    pub settings: Settings,
    pub initial_urls: Vec<URL_URL>,
    pub data_directory: PathBuf,
    pub locale: String,
    pub load_extensions: Vec<String>,
    pub auto_accept_extension_prompts: bool,
}

static STARTUP: OnceLock<Startup> = OnceLock::new();

pub fn set_startup(startup: Startup) {
    if STARTUP.set(startup).is_err() {
        panic!("The UI connected to the Hub twice");
    }
}

pub fn startup() -> &'static Startup {
    STARTUP.get().expect("the UI is connected to the Hub")
}

pub fn connection() -> &'static HubConnection {
    &startup().connection
}
