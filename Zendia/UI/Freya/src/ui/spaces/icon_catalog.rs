/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Searchable Emoji & SVG icon catalog for the Space Icon Picker (`ICO-01`).

/// A searchable emoji entry with display glyph and lowercase search keywords.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmojiEntry {
    pub glyph: &'static str,
    pub keywords: &'static str,
}

/// A searchable SVG icon entry backed by Zendia's bundled icon table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SvgIconEntry {
    pub icon_name: &'static str,
    pub keywords: &'static str,
}

/// Curated searchable emoji catalog matching Zen's emoji picker categories (faces, objects, travel, work, symbols).
pub const EMOJI_CATALOG: &[EmojiEntry] = &[
    EmojiEntry {
        glyph: "🚀",
        keywords: "rocket launch space startup fast",
    },
    EmojiEntry {
        glyph: "✨",
        keywords: "sparkles magic stars shine",
    },
    EmojiEntry {
        glyph: "🔥",
        keywords: "fire flame hot trending",
    },
    EmojiEntry {
        glyph: "💻",
        keywords: "laptop computer code dev work",
    },
    EmojiEntry {
        glyph: "📚",
        keywords: "books study library read docs",
    },
    EmojiEntry {
        glyph: "🎨",
        keywords: "palette art design paint creative",
    },
    EmojiEntry {
        glyph: "💼",
        keywords: "briefcase work business office",
    },
    EmojiEntry {
        glyph: "🏠",
        keywords: "home house personal",
    },
    EmojiEntry {
        glyph: "🌍",
        keywords: "globe earth world web",
    },
    EmojiEntry {
        glyph: "⚡",
        keywords: "lightning bolt fast power energy",
    },
    EmojiEntry {
        glyph: "💡",
        keywords: "bulb idea light inspiration",
    },
    EmojiEntry {
        glyph: "🎯",
        keywords: "target goal focus bullseye",
    },
    EmojiEntry {
        glyph: "🎵",
        keywords: "music note audio song",
    },
    EmojiEntry {
        glyph: "🎮",
        keywords: "game controller play gaming",
    },
    EmojiEntry {
        glyph: "📷",
        keywords: "camera photo picture media",
    },
    EmojiEntry {
        glyph: "🎬",
        keywords: "video movie film cinema",
    },
    EmojiEntry {
        glyph: "📧",
        keywords: "mail email inbox letter",
    },
    EmojiEntry {
        glyph: "💬",
        keywords: "chat speech bubble message",
    },
    EmojiEntry {
        glyph: "🔒",
        keywords: "lock security private safe",
    },
    EmojiEntry {
        glyph: "🔑",
        keywords: "key password access auth",
    },
    EmojiEntry {
        glyph: "⚙️",
        keywords: "gear settings config tools",
    },
    EmojiEntry {
        glyph: "🔧",
        keywords: "wrench tool fix build",
    },
    EmojiEntry {
        glyph: "🧪",
        keywords: "flask lab science test experiment",
    },
    EmojiEntry {
        glyph: "📊",
        keywords: "chart stats analytics graph data",
    },
    EmojiEntry {
        glyph: "💰",
        keywords: "money finance bank coins budget",
    },
    EmojiEntry {
        glyph: "🛒",
        keywords: "cart shop shopping store buy",
    },
    EmojiEntry {
        glyph: "✈️",
        keywords: "airplane travel flight trip",
    },
    EmojiEntry {
        glyph: "🧭",
        keywords: "compass explore navigate map",
    },
    EmojiEntry {
        glyph: "🌿",
        keywords: "leaf plant nature green eco",
    },
    EmojiEntry {
        glyph: "🌸",
        keywords: "flower blossom pink spring",
    },
    EmojiEntry {
        glyph: "☀️",
        keywords: "sun light day weather",
    },
    EmojiEntry {
        glyph: "🌙",
        keywords: "moon night dark sleep",
    },
    EmojiEntry {
        glyph: "⭐",
        keywords: "star favorite bookmark top",
    },
    EmojiEntry {
        glyph: "❤️",
        keywords: "heart love favorite like",
    },
    EmojiEntry {
        glyph: "💜",
        keywords: "purple heart love",
    },
    EmojiEntry {
        glyph: "💙",
        keywords: "blue heart love",
    },
    EmojiEntry {
        glyph: "💚",
        keywords: "green heart love",
    },
    EmojiEntry {
        glyph: "☕",
        keywords: "coffee cafe drink break",
    },
    EmojiEntry {
        glyph: "🍕",
        keywords: "pizza food eat lunch",
    },
    EmojiEntry {
        glyph: "🍔",
        keywords: "burger fast food eat",
    },
    EmojiEntry {
        glyph: "🍦",
        keywords: "ice cream dessert sweet",
    },
    EmojiEntry {
        glyph: "🐾",
        keywords: "paw pet dog cat animal",
    },
    EmojiEntry {
        glyph: "🐟",
        keywords: "fish ocean sea animal",
    },
    EmojiEntry {
        glyph: "🦋",
        keywords: "butterfly nature insect",
    },
    EmojiEntry {
        glyph: "🦊",
        keywords: "fox animal browser",
    },
    EmojiEntry {
        glyph: "🐱",
        keywords: "cat kitty pet",
    },
    EmojiEntry {
        glyph: "🐶",
        keywords: "dog puppy pet",
    },
    EmojiEntry {
        glyph: "🧠",
        keywords: "brain ai think smart mind",
    },
    EmojiEntry {
        glyph: "🤖",
        keywords: "robot agent ai bot",
    },
    EmojiEntry {
        glyph: "👾",
        keywords: "alien arcade pixel game",
    },
    EmojiEntry {
        glyph: "🎓",
        keywords: "school graduation university study",
    },
    EmojiEntry {
        glyph: "📝",
        keywords: "memo notes write document",
    },
    EmojiEntry {
        glyph: "📁",
        keywords: "folder files archive project",
    },
    EmojiEntry {
        glyph: "📌",
        keywords: "pin pushpin bookmark",
    },
    EmojiEntry {
        glyph: "🔔",
        keywords: "bell notification alert alarm",
    },
    EmojiEntry {
        glyph: "📣",
        keywords: "megaphone announce news",
    },
    EmojiEntry {
        glyph: "🎁",
        keywords: "gift present box reward",
    },
    EmojiEntry {
        glyph: "🎉",
        keywords: "tada party celebrate",
    },
    EmojiEntry {
        glyph: "🏆",
        keywords: "trophy win award prize",
    },
    EmojiEntry {
        glyph: "⚽",
        keywords: "soccer football sport",
    },
    EmojiEntry {
        glyph: "🏀",
        keywords: "basketball sport ball",
    },
    EmojiEntry {
        glyph: "⚾",
        keywords: "baseball sport ball",
    },
    EmojiEntry {
        glyph: "🏋️",
        keywords: "weight gym fitness workout",
    },
    EmojiEntry {
        glyph: "🚲",
        keywords: "bike bicycle ride",
    },
    EmojiEntry {
        glyph: "🚗",
        keywords: "car drive auto",
    },
    EmojiEntry {
        glyph: "🗺️",
        keywords: "map world location",
    },
    EmojiEntry {
        glyph: "📍",
        keywords: "pin location place",
    },
    EmojiEntry {
        glyph: "⏱️",
        keywords: "timer clock time schedule",
    },
    EmojiEntry {
        glyph: "📅",
        keywords: "calendar date schedule plan",
    },
    EmojiEntry {
        glyph: "🔖",
        keywords: "bookmark tag label",
    },
    EmojiEntry {
        glyph: "🧩",
        keywords: "puzzle extension plugin addon",
    },
    EmojiEntry {
        glyph: "🛡️",
        keywords: "shield protect privacy security",
    },
    EmojiEntry {
        glyph: "🔮",
        keywords: "crystal magic future",
    },
    EmojiEntry {
        glyph: "🌈",
        keywords: "rainbow colors gradient",
    },
    EmojiEntry {
        glyph: "💎",
        keywords: "gem diamond jewel",
    },
    EmojiEntry {
        glyph: "🪙",
        keywords: "coin crypto token",
    },
    EmojiEntry {
        glyph: "💳",
        keywords: "card credit payment wallet",
    },
    EmojiEntry {
        glyph: "🎟️",
        keywords: "ticket event pass",
    },
    EmojiEntry {
        glyph: "🎧",
        keywords: "headphones podcast audio music",
    },
    EmojiEntry {
        glyph: "🎤",
        keywords: "microphone voice record",
    },
    EmojiEntry {
        glyph: "📺",
        keywords: "tv television stream watch",
    },
    EmojiEntry {
        glyph: "📰",
        keywords: "news newspaper rss feed",
    },
    EmojiEntry {
        glyph: "☁️",
        keywords: "cloud weather storage sync",
    },
    EmojiEntry {
        glyph: "💧",
        keywords: "water drop blue",
    },
    EmojiEntry {
        glyph: "🌊",
        keywords: "wave ocean sea surf",
    },
    EmojiEntry {
        glyph: "🏔️",
        keywords: "mountain peak hike",
    },
    EmojiEntry {
        glyph: "🌴",
        keywords: "palm beach vacation",
    },
    EmojiEntry {
        glyph: "🪐",
        keywords: "planet saturn space cosmos",
    },
    EmojiEntry {
        glyph: "🌌",
        keywords: "galaxy stars universe",
    },
    EmojiEntry {
        glyph: "🔭",
        keywords: "telescope astronomy space",
    },
    EmojiEntry {
        glyph: "🔬",
        keywords: "microscope research science",
    },
    EmojiEntry {
        glyph: "🧬",
        keywords: "dna bio science",
    },
    EmojiEntry {
        glyph: "🩺",
        keywords: "health medical doctor",
    },
    EmojiEntry {
        glyph: "🍎",
        keywords: "apple fruit healthy",
    },
    EmojiEntry {
        glyph: "🍰",
        keywords: "cake dessert birthday",
    },
    EmojiEntry {
        glyph: "🍺",
        keywords: "beer drink social",
    },
];

/// Full bundled SVG icon catalog (86 selectable icons matching Zen's `SVG_ICONS` density and backed by Zendia's icon table).
pub const SVG_ICON_CATALOG: &[SvgIconEntry] = &[
    SvgIconEntry {
        icon_name: "briefcase",
        keywords: "briefcase work business office",
    },
    SvgIconEntry {
        icon_name: "book",
        keywords: "book read docs study",
    },
    SvgIconEntry {
        icon_name: "code",
        keywords: "code brackets dev programming",
    },
    SvgIconEntry {
        icon_name: "compass",
        keywords: "compass explore navigate",
    },
    SvgIconEntry {
        icon_name: "flame",
        keywords: "flame fire hot",
    },
    SvgIconEntry {
        icon_name: "heart",
        keywords: "heart love favorite",
    },
    SvgIconEntry {
        icon_name: "star",
        keywords: "star favorite bookmark",
    },
    SvgIconEntry {
        icon_name: "rocket",
        keywords: "rocket launch space",
    },
    SvgIconEntry {
        icon_name: "terminal",
        keywords: "terminal shell console cli",
    },
    SvgIconEntry {
        icon_name: "music",
        keywords: "music audio song",
    },
    SvgIconEntry {
        icon_name: "globe",
        keywords: "globe web internet world",
    },
    SvgIconEntry {
        icon_name: "home",
        keywords: "home house personal",
    },
    SvgIconEntry {
        icon_name: "mail",
        keywords: "mail email inbox",
    },
    SvgIconEntry {
        icon_name: "palette",
        keywords: "palette art color theme",
    },
    SvgIconEntry {
        icon_name: "sparkles",
        keywords: "sparkles magic ai",
    },
    SvgIconEntry {
        icon_name: "camera",
        keywords: "camera photo media",
    },
    SvgIconEntry {
        icon_name: "bolt",
        keywords: "bolt lightning fast",
    },
    SvgIconEntry {
        icon_name: "bookmark",
        keywords: "bookmark save",
    },
    SvgIconEntry {
        icon_name: "bookmark-hollow",
        keywords: "bookmark hollow outline",
    },
    SvgIconEntry {
        icon_name: "bookmark-star-on-tray",
        keywords: "bookmark tray star",
    },
    SvgIconEntry {
        icon_name: "brackets-curly",
        keywords: "brackets curly code json",
    },
    SvgIconEntry {
        icon_name: "check",
        keywords: "check done ok",
    },
    SvgIconEntry {
        icon_name: "circle-bars-filter",
        keywords: "filter sort bars",
    },
    SvgIconEntry {
        icon_name: "container-tab",
        keywords: "container profile tab",
    },
    SvgIconEntry {
        icon_name: "cookies-fill",
        keywords: "cookies site data",
    },
    SvgIconEntry {
        icon_name: "downloads",
        keywords: "downloads files save",
    },
    SvgIconEntry {
        icon_name: "duplicate-tab",
        keywords: "duplicate copy tab",
    },
    SvgIconEntry {
        icon_name: "edit-copy",
        keywords: "copy clipboard",
    },
    SvgIconEntry {
        icon_name: "edit-cut",
        keywords: "cut scissors",
    },
    SvgIconEntry {
        icon_name: "edit-delete",
        keywords: "delete remove",
    },
    SvgIconEntry {
        icon_name: "edit-paste",
        keywords: "paste clipboard",
    },
    SvgIconEntry {
        icon_name: "edit-theme",
        keywords: "theme edit brush",
    },
    SvgIconEntry {
        icon_name: "essential-add",
        keywords: "essential add star",
    },
    SvgIconEntry {
        icon_name: "essential-remove",
        keywords: "essential remove",
    },
    SvgIconEntry {
        icon_name: "expand-sidebar",
        keywords: "sidebar expand",
    },
    SvgIconEntry {
        icon_name: "extension",
        keywords: "extension puzzle plugin",
    },
    SvgIconEntry {
        icon_name: "extension-fill",
        keywords: "extension filled puzzle",
    },
    SvgIconEntry {
        icon_name: "eyedropper",
        keywords: "eyedropper color pick",
    },
    SvgIconEntry {
        icon_name: "face-sun",
        keywords: "sun light day bright",
    },
    SvgIconEntry {
        icon_name: "folder",
        keywords: "folder directory group",
    },
    SvgIconEntry {
        icon_name: "fullscreen",
        keywords: "fullscreen expand screen",
    },
    SvgIconEntry {
        icon_name: "hammer",
        keywords: "hammer build tool construct",
    },
    SvgIconEntry {
        icon_name: "heart-circle-fill",
        keywords: "heart circle favorite",
    },
    SvgIconEntry {
        icon_name: "help",
        keywords: "help question info",
    },
    SvgIconEntry {
        icon_name: "history",
        keywords: "history clock recent time",
    },
    SvgIconEntry {
        icon_name: "info",
        keywords: "info about details",
    },
    SvgIconEntry {
        icon_name: "library",
        keywords: "library archive books",
    },
    SvgIconEntry {
        icon_name: "library-downloads",
        keywords: "library downloads",
    },
    SvgIconEntry {
        icon_name: "library-history",
        keywords: "library history",
    },
    SvgIconEntry {
        icon_name: "library-media",
        keywords: "library media video",
    },
    SvgIconEntry {
        icon_name: "library-space-routing",
        keywords: "routing rules space",
    },
    SvgIconEntry {
        icon_name: "library-spaces",
        keywords: "spaces workspaces grid",
    },
    SvgIconEntry {
        icon_name: "lightbulb",
        keywords: "lightbulb idea lamp",
    },
    SvgIconEntry {
        icon_name: "link",
        keywords: "link chain url",
    },
    SvgIconEntry {
        icon_name: "manage",
        keywords: "manage config sliders",
    },
    SvgIconEntry {
        icon_name: "media-play",
        keywords: "play media video",
    },
    SvgIconEntry {
        icon_name: "media-pause",
        keywords: "pause media",
    },
    SvgIconEntry {
        icon_name: "media-unmute",
        keywords: "volume audio sound",
    },
    SvgIconEntry {
        icon_name: "media-mute",
        keywords: "mute silent audio",
    },
    SvgIconEntry {
        icon_name: "microphone",
        keywords: "microphone voice audio",
    },
    SvgIconEntry {
        icon_name: "microphone-fill",
        keywords: "microphone active voice",
    },
    SvgIconEntry {
        icon_name: "moon-stars",
        keywords: "moon night dark stars",
    },
    SvgIconEntry {
        icon_name: "move-cursor",
        keywords: "cursor move pointer",
    },
    SvgIconEntry {
        icon_name: "new-tab-image",
        keywords: "image picture photo",
    },
    SvgIconEntry {
        icon_name: "open",
        keywords: "open external window",
    },
    SvgIconEntry {
        icon_name: "page-portrait",
        keywords: "page document file",
    },
    SvgIconEntry {
        icon_name: "paintbrush",
        keywords: "paintbrush paint art",
    },
    SvgIconEntry {
        icon_name: "paintbrush-fill",
        keywords: "paintbrush filled art",
    },
    SvgIconEntry {
        icon_name: "passwords",
        keywords: "passwords key lock",
    },
    SvgIconEntry {
        icon_name: "permissions",
        keywords: "permissions shield",
    },
    SvgIconEntry {
        icon_name: "permissions-fill",
        keywords: "permissions filled",
    },
    SvgIconEntry {
        icon_name: "pin",
        keywords: "pin pinned tack",
    },
    SvgIconEntry {
        icon_name: "popup",
        keywords: "popup window",
    },
    SvgIconEntry {
        icon_name: "print",
        keywords: "print paper",
    },
    SvgIconEntry {
        icon_name: "private-window",
        keywords: "private incognito mask",
    },
    SvgIconEntry {
        icon_name: "reader-mode",
        keywords: "reader article text",
    },
    SvgIconEntry {
        icon_name: "save",
        keywords: "save disk download",
    },
    SvgIconEntry {
        icon_name: "screen",
        keywords: "screen monitor display",
    },
    SvgIconEntry {
        icon_name: "screenshot",
        keywords: "screenshot capture",
    },
    SvgIconEntry {
        icon_name: "search-glass",
        keywords: "search find glass",
    },
    SvgIconEntry {
        icon_name: "security",
        keywords: "security lock safe",
    },
    SvgIconEntry {
        icon_name: "settings",
        keywords: "settings gear preferences",
    },
    SvgIconEntry {
        icon_name: "share",
        keywords: "share send export",
    },
    SvgIconEntry {
        icon_name: "sliders",
        keywords: "sliders controls tune",
    },
    SvgIconEntry {
        icon_name: "split",
        keywords: "split layout panes",
    },
    SvgIconEntry {
        icon_name: "translations",
        keywords: "translations language globe",
    },
];

/// Filters emojis by case-insensitive substring match against keywords or glyph.
pub fn filter_emojis(query: &str) -> Vec<&'static EmojiEntry> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return EMOJI_CATALOG.iter().collect();
    }
    EMOJI_CATALOG
        .iter()
        .filter(|e| e.keywords.contains(&q) || e.glyph.contains(&q))
        .collect()
}

/// Filters SVG icons by case-insensitive substring match against icon name or keywords.
pub fn filter_svg_icons(query: &str) -> Vec<&'static SvgIconEntry> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return SVG_ICON_CATALOG.iter().collect();
    }
    SVG_ICON_CATALOG
        .iter()
        .filter(|e| e.icon_name.contains(&q) || e.keywords.contains(&q))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_catalog_has_86_svgs_and_searchable_emojis() {
        assert_eq!(SVG_ICON_CATALOG.len(), 86);
        assert!(EMOJI_CATALOG.len() >= 90);
        assert!(filter_emojis("rocket").iter().any(|e| e.glyph == "🚀"));
        assert!(filter_svg_icons("hammer").iter().any(|e| e.icon_name == "hammer"));
        assert!(filter_svg_icons("").len() == 86);
    }
}
