/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The keymap engine behind both keyboard schemes: standard browser shortcuts (the default) and vim (opt-in).
//!
//! It loads profiles (a base profile plus the user's own), layers them, and compiles the active one into a table. Both
//! InputServer and the UI process use this crate: InputServer builds the tables and handles counts, macros, the `:`
//! command line and profile scripts, and the UI process resolves ordinary keys from the table without waiting on IPC.
//!
//! See `Zendia/Documentation/Architecture.md`, "Keeping keys instant".
