/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The client side of the Automation World, the privileged isolated world inside each page.
//!
//! It finds, clicks, scrolls and reads elements, and gives them stable references. Vim hints and AI agents use the same
//! element references. Requests reach the page through the embedder channel.
//!
//! See `Zendia/Documentation/Architecture.md`, "Features".
