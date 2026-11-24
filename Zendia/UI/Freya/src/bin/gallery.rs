/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Standalone UI gallery binary running against `BrowserCore`'s stub backend with no Ladybird Hub.

use zendia::gallery::{GalleryLaunchOptions, launch_gallery};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{}", GalleryLaunchOptions::help_text());
        return;
    }
    let options = GalleryLaunchOptions::parse_args(args);
    launch_gallery(options);
}
