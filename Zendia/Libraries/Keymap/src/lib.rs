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
//! So far it has the key notation (`<C-t>`, `<C-S-Tab>`, `<A-Left>`, `<F5>`), TOML profiles compiled into a table,
//! the resolver for the standard scheme, and the vim key engine (modes, counts, sequences, `timeoutlen`). Layering
//! profiles comes with InputServer.
//!
//! See `Zendia/Documentation/Architecture.md`, "Keeping keys instant".

pub mod engine;
pub mod notation;
pub mod resolver;
pub mod table;

pub use engine::{DEFAULT_TIMEOUTLEN, KeyEngine, KeyMatch, ModeOptions, ModeTables, PendingKeys, UnknownMode};
pub use notation::{KeyCode, KeyModifiers, KeySequence, KeyStroke, NotationError};
pub use resolver::{KeyResolver, NextKeyHint, Resolution};
pub use table::{
    ArgValue, Binding, CHROME_MODES, CommandInvocation, GroupSpec, KeyTable, Profile, STANDARD_PROFILE_TOML,
    TableError, TrieChild, VIM_PROFILE_TOML,
};
