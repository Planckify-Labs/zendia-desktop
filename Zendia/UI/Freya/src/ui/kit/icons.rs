/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Embedded Zen/Zendia SVG icons and the [`Icon`] component.
//!
//! Temporary MPL-2.0 placeholder SVGs live unmodified under `Zendia/Base/res/icons/zen/` and are embedded at compile
//! time. Mozilla preprocessor comment lines and `context-fill` / `context-stroke` attributes are normalized into
//! standard `currentColor` SVG bytes before rasterization so every icon inherits its [`ThemeRole`] color.

use crate::ui::theme::{ThemeRole, use_theme};
use freya::prelude::*;
use std::collections::HashMap;
use std::sync::LazyLock;

macro_rules! zen_icon {
    ($name:literal, $rel_path:literal) => {
        (
            $name,
            include_bytes!(concat!("../../../../../Base/res/icons/zen/", $rel_path)) as &'static [u8],
        )
    };
}

/// Canonical table mapping each embedded Zen/Zendia icon name to its raw SVG bytes.
pub const ICON_TABLE: &[(&str, &[u8])] = &[
    zen_icon!("arrow-corner-down-right", "arrow-corner-down-right.svg"),
    zen_icon!("arrow-down", "arrow-down.svg"),
    zen_icon!("arrow-left", "arrow-left.svg"),
    zen_icon!("arrow-right", "arrow-right.svg"),
    zen_icon!("arrow-rotate-anticlockwise", "arrow-rotate-anticlockwise.svg"),
    zen_icon!("arrow-up", "arrow-up.svg"),
    zen_icon!("arrows-collapse", "arrows-collapse.svg"),
    zen_icon!("arrows-expand", "arrows-expand.svg"),
    zen_icon!("back", "back.svg"),
    zen_icon!("block", "block.svg"),
    zen_icon!("bolt", "bolt.svg"),
    zen_icon!("bookmark", "bookmark.svg"),
    zen_icon!("bookmark-hollow", "bookmark-hollow.svg"),
    zen_icon!("bookmark-star-on-tray", "bookmark-star-on-tray.svg"),
    zen_icon!("boost", "boost.svg"),
    zen_icon!("brackets-curly", "brackets-curly.svg"),
    zen_icon!("camera", "camera.svg"),
    zen_icon!("check", "check.svg"),
    zen_icon!("chevron", "chevron.svg"),
    zen_icon!("circle-bars-filter", "circle-bars-filter.svg"),
    zen_icon!("close", "close.svg"),
    zen_icon!("container-tab", "container-tab.svg"),
    zen_icon!("cookies-fill", "cookies-fill.svg"),
    zen_icon!("dart-down", "dart-down.svg"),
    zen_icon!("downloads", "downloads.svg"),
    zen_icon!("drag-indicator", "drag-indicator.svg"),
    zen_icon!("duplicate-tab", "duplicate-tab.svg"),
    zen_icon!("edit-copy", "edit-copy.svg"),
    zen_icon!("edit-cut", "edit-cut.svg"),
    zen_icon!("edit-delete", "edit-delete.svg"),
    zen_icon!("edit-paste", "edit-paste.svg"),
    zen_icon!("edit-theme", "edit-theme.svg"),
    zen_icon!("essential-add", "essential-add.svg"),
    zen_icon!("essential-remove", "essential-remove.svg"),
    zen_icon!("expand-sidebar", "expand-sidebar.svg"),
    zen_icon!("extension", "extension.svg"),
    zen_icon!("extension-fill", "extension-fill.svg"),
    zen_icon!("eyedropper", "eyedropper.svg"),
    zen_icon!("face-sun", "face-sun.svg"),
    zen_icon!("folder", "folder.svg"),
    zen_icon!("forward", "forward.svg"),
    zen_icon!("fullscreen", "fullscreen.svg"),
    zen_icon!("fullscreen-exit", "fullscreen-exit.svg"),
    zen_icon!("hammer", "hammer.svg"),
    zen_icon!("heart-circle-fill", "heart-circle-fill.svg"),
    zen_icon!("help", "help.svg"),
    zen_icon!("history", "history.svg"),
    zen_icon!("home", "home.svg"),
    zen_icon!("info", "info.svg"),
    zen_icon!("library", "library.svg"),
    zen_icon!("lightbulb", "lightbulb.svg"),
    zen_icon!("link", "link.svg"),
    zen_icon!("mail", "mail.svg"),
    zen_icon!("manage", "manage.svg"),
    zen_icon!("media-mute", "media-mute.svg"),
    zen_icon!("media-next", "media-next.svg"),
    zen_icon!("media-pause", "media-pause.svg"),
    zen_icon!("media-play", "media-play.svg"),
    zen_icon!("media-previous", "media-previous.svg"),
    zen_icon!("media-unmute", "media-unmute.svg"),
    zen_icon!("menu", "menu.svg"),
    zen_icon!("microphone", "microphone.svg"),
    zen_icon!("microphone-blocked", "microphone-blocked.svg"),
    zen_icon!("microphone-fill", "microphone-fill.svg"),
    zen_icon!("moon-stars", "moon-stars.svg"),
    zen_icon!("move-cursor", "move-cursor.svg"),
    zen_icon!("new-tab-image", "new-tab-image.svg"),
    zen_icon!("open", "open.svg"),
    zen_icon!("page-portrait", "page-portrait.svg"),
    zen_icon!("paintbrush", "paintbrush.svg"),
    zen_icon!("paintbrush-fill", "paintbrush-fill.svg"),
    zen_icon!("palette", "palette.svg"),
    zen_icon!("passwords", "passwords.svg"),
    zen_icon!("permissions", "permissions.svg"),
    zen_icon!("permissions-fill", "permissions-fill.svg"),
    zen_icon!("pin", "pin.svg"),
    zen_icon!("plus", "plus.svg"),
    zen_icon!("popup", "popup.svg"),
    zen_icon!("print", "print.svg"),
    zen_icon!("private-window", "private-window.svg"),
    zen_icon!("private-window-small", "private-window-small.svg"),
    zen_icon!("reader-mode", "reader-mode.svg"),
    zen_icon!("reload", "reload.svg"),
    zen_icon!("save", "save.svg"),
    zen_icon!("screen", "screen.svg"),
    zen_icon!("screenshot", "screenshot.svg"),
    zen_icon!("search-glass", "search-glass.svg"),
    zen_icon!("search-page", "search-page.svg"),
    zen_icon!("security", "security.svg"),
    zen_icon!("security-broken", "security-broken.svg"),
    zen_icon!("security-broken-fill", "security-broken-fill.svg"),
    zen_icon!("security-fill", "security-fill.svg"),
    zen_icon!("security-warning", "security-warning.svg"),
    zen_icon!("settings", "settings.svg"),
    zen_icon!("share", "share.svg"),
    zen_icon!("sidebar", "sidebar.svg"),
    zen_icon!("sidebar-right", "sidebar-right.svg"),
    zen_icon!("sidebars-right", "sidebars-right.svg"),
    zen_icon!("sliders", "sliders.svg"),
    zen_icon!("sparkles", "sparkles.svg"),
    zen_icon!("split", "split.svg"),
    zen_icon!("tab", "tab.svg"),
    zen_icon!("tab-audio-blocked-small", "tab-audio-blocked-small.svg"),
    zen_icon!("tab-audio-muted-small", "tab-audio-muted-small.svg"),
    zen_icon!("tab-audio-playing-small", "tab-audio-playing-small.svg"),
    zen_icon!("tracking-protection", "tracking-protection.svg"),
    zen_icon!("tracking-protection-fill", "tracking-protection-fill.svg"),
    zen_icon!("translations", "translations.svg"),
    zen_icon!("trash", "trash.svg"),
    zen_icon!("u-turn-to-left", "u-turn-to-left.svg"),
    zen_icon!("unpin", "unpin.svg"),
    zen_icon!("urlbar-arrow", "urlbar-arrow.svg"),
    zen_icon!("window", "window.svg"),
    zen_icon!("zoom-out", "zoom-out.svg"),
    // Selectable space and Zendia toolbar icons.
    zen_icon!("selectable/airplane", "selectable/airplane.svg"),
    zen_icon!("selectable/bell", "selectable/bell.svg"),
    zen_icon!("selectable/book", "selectable/book.svg"),
    zen_icon!("selectable/briefcase", "selectable/briefcase.svg"),
    zen_icon!("selectable/chat", "selectable/chat.svg"),
    zen_icon!("selectable/cloud", "selectable/cloud.svg"),
    zen_icon!("selectable/code", "selectable/code.svg"),
    zen_icon!("selectable/eye", "selectable/eye.svg"),
    zen_icon!("selectable/flame", "selectable/flame.svg"),
    zen_icon!("selectable/folder", "selectable/folder.svg"),
    zen_icon!("selectable/globe", "selectable/globe.svg"),
    zen_icon!("selectable/heart", "selectable/heart.svg"),
    zen_icon!("selectable/key", "selectable/key.svg"),
    zen_icon!("selectable/layers", "selectable/layers.svg"),
    zen_icon!("selectable/lock-closed", "selectable/lock-closed.svg"),
    zen_icon!("selectable/logo-github", "selectable/logo-github.svg"),
    zen_icon!("selectable/logo-rss", "selectable/logo-rss.svg"),
    zen_icon!("selectable/mail", "selectable/mail.svg"),
    zen_icon!("selectable/music", "selectable/music.svg"),
    zen_icon!("selectable/people", "selectable/people.svg"),
    zen_icon!("selectable/rocket", "selectable/rocket.svg"),
    zen_icon!("selectable/star", "selectable/star.svg"),
    zen_icon!("selectable/terminal", "selectable/terminal.svg"),
    zen_icon!("selectable/warning", "selectable/warning.svg"),
];

/// Semantic aliases used across Zendia's contributions, sample model, and surfaces.
const ICON_ALIASES: &[(&str, &str)] = &[
    ("chat", "selectable/chat"),
    ("agents", "selectable/layers"),
    ("layers", "selectable/layers"),
    ("webchat", "selectable/globe"),
    ("globe", "selectable/globe"),
    ("compass", "selectable/globe"),
    ("people", "selectable/people"),
    ("accounts", "selectable/people"),
    ("terminal", "selectable/terminal"),
    ("vim", "selectable/terminal"),
    ("briefcase", "selectable/briefcase"),
    ("book", "selectable/book"),
    ("code", "selectable/code"),
    ("star", "selectable/star"),
    ("warning", "selectable/warning"),
    ("bell", "selectable/bell"),
    ("cloud", "selectable/cloud"),
    ("eye", "selectable/eye"),
    ("flame", "selectable/flame"),
    ("heart", "selectable/heart"),
    ("key", "selectable/key"),
    ("lock", "selectable/lock-closed"),
    ("music", "selectable/music"),
    ("rocket", "selectable/rocket"),
    ("github", "selectable/logo-github"),
    ("rss", "selectable/logo-rss"),
    ("airplane", "selectable/airplane"),
    ("layout-float", "popup"),
    ("layout-slide", "sidebar-right"),
    ("layout-split", "split"),
    ("new-tab", "new-tab-image"),
    ("search", "search-glass"),
    ("copy", "edit-copy"),
    ("cut", "edit-cut"),
    ("paste", "edit-paste"),
    ("delete", "trash"),
    ("sun", "face-sun"),
    ("moon", "moon-stars"),
    ("audio-playing", "tab-audio-playing-small"),
    ("audio-muted", "tab-audio-muted-small"),
    ("audio-blocked", "tab-audio-blocked-small"),
    ("put-back", "arrow-corner-down-right"),
    ("captcha", "security"),
];

/// Resolves an icon name or alias to its canonical entry key in [`ICON_TABLE`].
pub fn canonical_icon_name(name: &str) -> Option<&'static str> {
    let trimmed = name.trim().strip_suffix(".svg").unwrap_or(name.trim());
    if let Some((canonical, _)) = ICON_TABLE.iter().find(|(key, _)| *key == trimmed) {
        return Some(*canonical);
    }
    if let Some((_, target)) = ICON_ALIASES.iter().find(|(alias, _)| *alias == trimmed) {
        return ICON_TABLE.iter().find(|(key, _)| key == target).map(|(key, _)| *key);
    }
    None
}

/// Looks up the raw embedded SVG bytes for an icon name or alias.
pub fn icon_raw_bytes(name: &str) -> Option<&'static [u8]> {
    let canonical = canonical_icon_name(name)?;
    ICON_TABLE
        .iter()
        .find(|(key, _)| *key == canonical)
        .map(|(_, bytes)| *bytes)
}

/// Strips Firefox preprocessor `#` lines and rewrites Mozilla `context-fill` / `context-stroke` tokens into standard
/// `currentColor` SVG so Skia renders the icon in the requested theme color.
pub fn prepare_svg_for_rendering(raw: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(raw);
    let mut cleaned = String::with_capacity(text.len());
    let mut past_preamble = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if !past_preamble {
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            past_preamble = true;
        }
        cleaned.push_str(line);
        cleaned.push('\n');
    }

    cleaned
        .replace("context-fill-opacity", "1")
        .replace("context-stroke-opacity", "1")
        .replace("context-fill", "currentColor")
        .replace("context-stroke", "currentColor")
        .into_bytes()
}

static PREPARED_ICONS: LazyLock<HashMap<&'static str, Bytes>> = LazyLock::new(|| {
    ICON_TABLE
        .iter()
        .map(|(name, raw)| (*name, Bytes::from(prepare_svg_for_rendering(raw))))
        .collect()
});

/// Returns cached, preprocessed SVG bytes ready for Freya's [`SvgViewer`].
pub fn prepared_icon_bytes(name: &str) -> Option<(&'static str, Bytes)> {
    let canonical = canonical_icon_name(name)?;
    PREPARED_ICONS.get(canonical).cloned().map(|bytes| (canonical, bytes))
}

/// Vector icon component rendered with Freya's [`SvgViewer`], sized and tinted by [`ThemeRole`].
#[derive(Clone, PartialEq)]
pub struct Icon {
    name: String,
    size: f32,
    role: ThemeRole,
    key: DiffKey,
}

impl KeyExt for Icon {
    fn key_mut(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Icon {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            size: 16.0,
            role: ThemeRole::Text,
            key: DiffKey::None,
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn role(mut self, role: ThemeRole) -> Self {
        self.role = role;
        self
    }
}

impl Component for Icon {
    fn render(&self) -> impl IntoElement {
        let theme = use_theme();
        let color = theme.color(self.role);
        let size = self.size.max(1.0);

        if let Some((canonical, bytes)) = prepared_icon_bytes(&self.name) {
            SvgViewer::new((("zendia-icon", canonical), bytes))
                .width(Size::px(size))
                .height(Size::px(size))
                .color(color)
                .show_loader(false)
                .into_element()
        } else {
            // Unknown icon placeholder so callers never panic on a missing plugin icon name.
            rect()
                .width(Size::px(size))
                .height(Size::px(size))
                .corner_radius(CornerRadius::new_all(theme.radii.xs))
                .border(Border::new().width(1.0).fill(color))
                .into_element()
        }
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use freya::engine::prelude::{FontMgr, svg};

    #[test]
    fn preprocessor_strips_mozilla_headers_and_rewrites_context_fill() {
        let sample = b"#filter dumbComments emptyLines substitution\n# License header\n<svg xmlns=\"http://www.w3.org/2000/svg\" fill=\"context-fill\" fill-opacity=\"context-fill-opacity\" viewBox=\"0 0 18 18\"><path d=\"M0 0h18v18H0z\"/></svg>";
        let prepared = String::from_utf8(prepare_svg_for_rendering(sample)).unwrap();
        assert!(prepared.starts_with("<svg"));
        assert!(!prepared.contains("#filter"));
        assert!(!prepared.contains("context-fill"));
        assert!(prepared.contains("fill=\"currentColor\""));
        assert!(prepared.contains("fill-opacity=\"1\""));
    }

    #[test]
    fn every_embedded_icon_prepares_and_parses_in_skia() {
        assert!(!ICON_TABLE.is_empty());
        for (name, raw) in ICON_TABLE {
            let prepared = prepare_svg_for_rendering(raw);
            let as_str = std::str::from_utf8(&prepared).unwrap_or_else(|e| panic!("icon `{name}` invalid utf-8: {e}"));
            let trimmed = as_str.trim_start();
            assert!(
                trimmed.starts_with("<svg") || trimmed.starts_with("<?xml") || trimmed.starts_with("<!--"),
                "icon `{name}` did not strip preprocessor preamble: {as_str}"
            );
            assert!(
                !as_str.contains("context-fill")
                    && !as_str.contains("context-stroke")
                    && !as_str.contains("currentColor-opacity"),
                "icon `{name}` still contains unresolved Mozilla context paint"
            );
            let dom = svg::Dom::from_bytes(&prepared, FontMgr::empty());
            assert!(
                dom.is_ok(),
                "Skia failed to parse embedded icon `{name}`: {:?}",
                dom.err()
            );
        }
    }

    #[test]
    fn zen_and_zendia_icon_aliases_resolve() {
        for name in [
            "back",
            "forward",
            "reload",
            "close",
            "plus",
            "chat",
            "agents",
            "webchat",
            "layout-float",
            "layout-slide",
            "layout-split",
            "vim",
            "briefcase",
            "book",
            "code",
            "compass",
            "folder",
            "search",
            "warning",
            "back.svg",
            "selectable/chat.svg",
        ] {
            assert!(
                icon_raw_bytes(name).is_some(),
                "expected icon `{name}` to resolve in ICON_TABLE"
            );
        }
        assert!(icon_raw_bytes("definitely-not-an-icon").is_none());
    }
}
