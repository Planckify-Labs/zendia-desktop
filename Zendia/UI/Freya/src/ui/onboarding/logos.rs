/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Vector icons for onboarding controls (sound, pause/play), search engines, importable browsers, and the nine preset
//! Essential apps. Also paints crisp Skia versions of the brand marks inside 2D vector scenes.

use freya::engine::prelude as sk;
use freya::prelude::*;

const SOUND_ON_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5L6 9H2v6h4l5 4V5z"/><path d="M15.5 8.5a5 5 0 0 1 0 7"/><path d="M19 5a10 10 0 0 1 0 14"/></svg>"#;
const SOUND_OFF_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5L6 9H2v6h4l5 4V5z"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/></svg>"#;
const PAUSE_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 14 14"><rect x="2.6" y="2.2" width="3" height="9.6" rx="1.2" fill="currentColor"/><rect x="8.4" y="2.2" width="3" height="9.6" rx="1.2" fill="currentColor"/></svg>"#;
const PLAY_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 14 14"><path d="M3.2 2.2L11.6 7L3.2 11.8Z" fill="currentColor"/></svg>"#;

const CHROME_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#FBBC05"/><path d="M12 2A10 10 0 0 1 20.66 7H12a5 5 0 0 0-4.33 2.5L3.34 7A10 10 0 0 1 12 2Z" fill="#EA4335"/><path d="M3.34 7L7.67 14.5A5 5 0 0 0 12 17v5A10 10 0 0 1 3.34 7Z" fill="#34A853"/><circle cx="12" cy="12" r="5" fill="#FFFFFF"/><circle cx="12" cy="12" r="3.8" fill="#4285F4"/></svg>"##;
const FIREFOX_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9.5" fill="#592ACB"/><path d="M20.5 11.5C20.5 16.5 16.7 20.5 11.8 20.5C7 20.5 3.5 16.8 3.5 12C3.5 8.5 5.6 5.6 8.5 4.4C7.3 6.2 7.5 8.4 9.1 9.5C9.4 7.7 11.1 6.6 12.8 6.9C11.4 8.1 11.6 10.3 13.3 10.9C15.1 11.5 16.6 10.1 16.3 8.2C18.8 8.9 20.5 10 20.5 11.5Z" fill="#FF7139"/><circle cx="13.2" cy="13.2" r="3.4" fill="#FFD567"/></svg>"##;

const GOOGLE_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M21.6 12.23c0-.71-.06-1.4-.18-2.05H12v3.88h5.39a4.6 4.6 0 0 1-2 3.02v2.5h3.24c1.9-1.75 2.97-4.32 2.97-7.35z" fill="#4285F4"/><path d="M12 22c2.7 0 4.96-.9 6.62-2.42l-3.24-2.5c-.9.6-2.04.95-3.38.95-2.6 0-4.8-1.76-5.59-4.12H3.06v2.58A10 10 0 0 0 12 22z" fill="#34A853"/><path d="M6.41 13.91a6 6 0 0 1 0-3.82V7.51H3.06a10 10 0 0 0 0 8.98l3.35-2.58z" fill="#FBBC05"/><path d="M12 5.97c1.47 0 2.79.51 3.83 1.5l2.87-2.87C16.95 2.99 14.7 2 12 2A10 10 0 0 0 3.06 7.51l3.35 2.58C7.2 7.73 9.4 5.97 12 5.97z" fill="#EA4335"/></svg>"##;
const BING_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M5.5 3.2L9.4 4.6V15.4L15.1 12.1L12.6 10.9L11.2 7.5L19.1 10.3V14.4L9.4 20L5.5 17.8V3.2Z" fill="#008373"/><path d="M9.4 15.4L19.1 10.3V14.4L9.4 20V15.4Z" fill="#258FFA"/></svg>"##;
const DUCKDUCKGO_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#DE5833"/><circle cx="11.5" cy="11" r="5.5" fill="#FFFFFF"/><circle cx="10.3" cy="9.8" r="1" fill="#1F2335"/><circle cx="13.2" cy="9.8" r="1" fill="#1F2335"/><path d="M13.5 11.8C16 11.3 18.2 12 18.2 13.1C18.2 14.1 15.9 14.6 13.4 14.1Z" fill="#F5B82E"/><path d="M8.5 17.2L11.5 15.8L14.5 17.2L11.5 18.8Z" fill="#5CB85C"/></svg>"##;

const OBSIDIAN_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M14.8 2.2L6.8 6.4L4.2 15.2L9.8 21.8L18.4 18.6L19.8 9.2L14.8 2.2Z" fill="#7C3AED"/><path d="M14.8 2.2L11.2 11.8L18.4 18.6L19.8 9.2L14.8 2.2Z" fill="#A78BFA"/><path d="M6.8 6.4L11.2 11.8L9.8 21.8L4.2 15.2L6.8 6.4Z" fill="#5B21B6"/></svg>"##;
const DISCORD_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M19.5 5.8A16.5 16.5 0 0 0 15.4 4.5L14.9 5.5A15.2 15.2 0 0 0 9.1 5.5L8.6 4.5A16.5 16.5 0 0 0 4.5 5.8C1.9 9.7 1.2 13.5 1.6 17.2A16.8 16.8 0 0 0 6.6 19.7L7.6 18.1A10.7 10.7 0 0 1 6 17.3L6.4 17A11.8 11.8 0 0 0 17.6 17L18 17.3A10.7 10.7 0 0 1 16.4 18.1L17.4 19.7A16.8 16.8 0 0 0 22.4 17.2C22.9 12.9 21.6 9.1 19.5 5.8Z" fill="#5865F2"/><circle cx="9" cy="13" r="1.7" fill="#FFFFFF"/><circle cx="15" cy="13" r="1.7" fill="#FFFFFF"/></svg>"##;
const TRELLO_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="2.5" y="2.5" width="19" height="19" rx="3.5" fill="#0079BF"/><rect x="5.5" y="5.5" width="5.2" height="11.8" rx="1.4" fill="#FFFFFF"/><rect x="13.3" y="5.5" width="5.2" height="7.6" rx="1.4" fill="#FFFFFF"/></svg>"##;
const SLACK_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="9.8" y="3" width="3.2" height="7.8" rx="1.6" fill="#2EB67D"/><circle cx="7.4" cy="9.2" r="1.6" fill="#36C5F0"/><rect x="3" y="9.8" width="7.8" height="3.2" rx="1.6" fill="#36C5F0"/><circle cx="14.8" cy="7.4" r="1.6" fill="#2EB67D"/><rect x="13.2" y="11" width="7.8" height="3.2" rx="1.6" fill="#ECB22E"/><circle cx="16.6" cy="14.8" r="1.6" fill="#ECB22E"/><rect x="11" y="13.2" width="3.2" height="7.8" rx="1.6" fill="#E01E5A"/><circle cx="9.2" cy="16.6" r="1.6" fill="#E01E5A"/></svg>"##;
const GITHUB_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill-rule="evenodd" clip-rule="evenodd" d="M12 2C6.48 2 2 6.59 2 12.25c0 4.53 2.87 8.37 6.84 9.73.5.09.68-.22.68-.49 0-.24-.01-.88-.01-1.73-2.78.62-3.37-1.37-3.37-1.37-.45-1.18-1.11-1.49-1.11-1.49-.91-.64.07-.63.07-.63 1 .07 1.53 1.06 1.53 1.06.89 1.57 2.34 1.12 2.91.85.09-.66.35-1.12.63-1.37-2.22-.26-4.56-1.14-4.56-5.07 0-1.12.39-2.03 1.03-2.75-.1-.26-.45-1.3.1-2.71 0 0 .84-.28 2.75 1.05A9.38 9.38 0 0 1 12 6.84c.85 0 1.7.12 2.5.35 1.91-1.33 2.75-1.05 2.75-1.05.55 1.41.2 2.45.1 2.71.64.72 1.03 1.63 1.03 2.75 0 3.94-2.34 4.81-4.57 5.06.36.32.68.94.68 1.9 0 1.37-.01 2.48-.01 2.82 0 .27.18.59.69.49A10.26 10.26 0 0 0 22 12.25C22 6.59 17.52 2 12 2Z" fill="#1F2328"/></svg>"##;
const TUTA_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M4.5 3.5H19.5C20.6 3.5 21.5 4.4 21.5 5.5V16.8C21.5 17.9 20.6 18.8 19.5 18.8H11.5L6.2 21.4V18.8H4.5C3.4 18.8 2.5 17.9 2.5 16.8V5.5C2.5 4.4 3.4 3.5 4.5 3.5Z" fill="#840010"/><path d="M5.5 6.8L18.5 6.8L12 12.6L5.5 6.8Z" fill="#FFFFFF"/><path d="M4.5 17.2L19.5 7.5V16.8C19.5 17.4 19 17.8 18.4 17.8H4.5Z" fill="#C00002"/></svg>"##;
const NOTION_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="3" y="3" width="18" height="18" rx="3.2" fill="#FFFFFF" stroke="#1F2335" stroke-width="2"/><path d="M8.2 16.8V7.4H10.1L14.2 13.7V7.4H16V16.8H14.2L10 10.3V16.8H8.2Z" fill="#1F2335"/></svg>"##;
const CALENDAR_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="17" rx="3" fill="#4285F4"/><rect x="6.5" y="7.5" width="11" height="10" rx="1.5" fill="#FFFFFF"/><path d="M16.5 21L21 16.5V18C21 19.66 19.66 21 18 21H16.5Z" fill="#EA4335"/><rect x="3" y="16.5" width="4.5" height="4.5" rx="1" fill="#34A853"/><rect x="16.5" y="4" width="4.5" height="4.5" rx="1" fill="#FBBC05"/><path d="M9.6 10.8H12.3L10.8 15.2M13.6 10.8H14.8V15.2" fill="none" stroke="#1A73E8" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const FIGMA_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M8.5 2.5H12V9H8.5A3.25 3.25 0 0 1 8.5 2.5Z" fill="#F24E1E"/><path d="M12 2.5H15.5A3.25 3.25 0 0 1 15.5 9H12V2.5Z" fill="#FF7262"/><path d="M12 9H15.5A3.25 3.25 0 1 1 15.5 15.5H12V9Z" fill="#1ABCFE"/><path d="M8.5 9H12V15.5H8.5A3.25 3.25 0 0 1 8.5 9Z" fill="#A259FF"/><path d="M8.5 15.5H12V18.75A3.25 3.25 0 1 1 8.5 15.5Z" fill="#0ACF83"/></svg>"##;
const GENERIC_GLOBE_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="8.5" fill="none" stroke="#4285F4" stroke-width="2"/><path d="M3.5 12H20.5M12 3.5C14.8 6.5 14.8 17.5 12 20.5M12 3.5C9.2 6.5 9.2 17.5 12 20.5" fill="none" stroke="#4285F4" stroke-width="1.6"/></svg>"##;

pub fn sound_icon(muted: bool, size: f32, color: Color) -> Element {
    let (key, bytes) = if muted {
        ("zendia-onboarding-sound-off", SOUND_OFF_SVG)
    } else {
        ("zendia-onboarding-sound-on", SOUND_ON_SVG)
    };
    SvgViewer::new((key, Bytes::from_static(bytes)))
        .width(Size::px(size))
        .height(Size::px(size))
        .color(color)
        .show_loader(false)
        .into_element()
}

pub fn playback_icon(paused: bool, size: f32, color: Color) -> Element {
    let (key, bytes) = if paused {
        ("zendia-onboarding-play", PLAY_SVG)
    } else {
        ("zendia-onboarding-pause", PAUSE_SVG)
    };
    SvgViewer::new((key, Bytes::from_static(bytes)))
        .width(Size::px(size))
        .height(Size::px(size))
        .color(color)
        .show_loader(false)
        .into_element()
}

/// Returns `(cache_key, svg_bytes)` for a search engine, import browser, or Essential app icon id.
pub fn brand_svg_bytes(id: &str) -> (&'static str, &'static [u8]) {
    match id.to_ascii_lowercase().as_str() {
        "chrome" => ("zendia-onb-chrome", CHROME_SVG),
        "firefox" => ("zendia-onb-firefox", FIREFOX_SVG),
        "google" => ("zendia-onb-google", GOOGLE_SVG),
        "bing" => ("zendia-onb-bing", BING_SVG),
        "duckduckgo" => ("zendia-onb-ddg", DUCKDUCKGO_SVG),
        "obsidian" => ("zendia-onb-obsidian", OBSIDIAN_SVG),
        "discord" => ("zendia-onb-discord", DISCORD_SVG),
        "trello" => ("zendia-onb-trello", TRELLO_SVG),
        "slack" => ("zendia-onb-slack", SLACK_SVG),
        "github" => ("zendia-onb-github", GITHUB_SVG),
        "tuta" => ("zendia-onb-tuta", TUTA_SVG),
        "notion" => ("zendia-onb-notion", NOTION_SVG),
        "calendar" | "google calendar" => ("zendia-onb-calendar", CALENDAR_SVG),
        "figma" => ("zendia-onb-figma", FIGMA_SVG),
        _ => ("zendia-onb-generic", GENERIC_GLOBE_SVG),
    }
}

/// Renders a crisp full-color brand SVG emblem inside Freya UI controls.
pub fn brand_logo_element(id: &str, size: f32) -> Element {
    let (key, bytes) = brand_svg_bytes(id);
    SvgViewer::new((key, Bytes::from_static(bytes)))
        .width(Size::px(size))
        .height(Size::px(size))
        .show_loader(false)
        .into_element()
}

/// Renders one of the onboarding brand SVGs directly onto a Skia [`sk::Canvas`] at `(x, y)` with side length `size`.
pub fn paint_brand_svg_on_canvas(canvas: &sk::Canvas, id: &str, x: f32, y: f32, size: f32) {
    if size <= 0.0 {
        return;
    }
    let (_, bytes) = brand_svg_bytes(id);
    let Ok(mut dom) = sk::svg::Dom::from_bytes(bytes, sk::FontMgr::empty()) else {
        return;
    };
    let save = canvas.save();
    canvas.translate((x, y));
    canvas.scale((size / 24.0, size / 24.0));
    dom.set_container_size(sk::Size::new(24.0, 24.0));
    dom.render(canvas);
    canvas.restore_to_count(save);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_onboarding_svg_parses_in_skia() {
        let font_mgr = sk::FontMgr::empty();
        for (name, bytes) in [
            ("sound-on", SOUND_ON_SVG),
            ("sound-off", SOUND_OFF_SVG),
            ("pause", PAUSE_SVG),
            ("play", PLAY_SVG),
        ] {
            assert!(
                sk::svg::Dom::from_bytes(bytes, font_mgr.clone()).is_ok(),
                "Skia failed to parse `{name}`"
            );
        }
        for id in [
            "chrome",
            "firefox",
            "google",
            "bing",
            "duckduckgo",
            "obsidian",
            "discord",
            "trello",
            "slack",
            "github",
            "tuta",
            "notion",
            "calendar",
            "figma",
            "other",
        ] {
            let (_, bytes) = brand_svg_bytes(id);
            assert!(
                sk::svg::Dom::from_bytes(bytes, font_mgr.clone()).is_ok(),
                "Skia failed to parse brand SVG `{id}`"
            );
        }
    }
}
