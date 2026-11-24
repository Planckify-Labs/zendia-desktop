/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The browser core, which runs in the UI process.
//!
//! It holds the command registry, through which every caller (the window, InputServer, PluginHost, ExtensionServer and
//! AgentServer) drives the browser; the browser model (windows and tabs); the event bus that tells subscribers what
//! changed; and the profile data (history, bookmarks, sessions). It runs with or without a window, so agents can work
//! headless.
//!
//! See `Zendia/Documentation/Architecture.md`, "The command layer".
