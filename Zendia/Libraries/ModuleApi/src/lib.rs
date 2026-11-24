/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The Module API: the contract through which PluginHost and ExtensionServer add to the browser.
//!
//! Modules contribute commands, toolbar items, menu and palette entries, keymaps, panels and agent tools, and subscribe
//! to events. UI is described as data (a declarative tree) that the Freya view draws, so this crate exposes no Freya
//! types and modules build without Freya.
//!
//! See `Zendia/Documentation/Architecture.md`, "Shared hooks".
