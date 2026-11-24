/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The template every Zendia service process is built on: its LibIPC endpoint, being launched by the UI process,
//! entering its sandbox, and being restarted after a crash. Each new process should cost little more than its own
//! logic.
//!
//! See `Zendia/Documentation/Architecture.md`, "Processes".
