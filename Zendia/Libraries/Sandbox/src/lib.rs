/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The sandbox profiles of Zendia's service processes: seccomp and Landlock on Linux, Seatbelt on macOS.
//!
//! Each service enters its profile as it starts, before it reads any untrusted input. PluginHost gets the strictest
//! profile, then ExtensionServer and AgentServer; InputServer gets a light one.
//!
//! See `Zendia/Documentation/Architecture.md`, "Sandboxing".
