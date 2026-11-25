/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Per-site security state, permissions, tracking protection, site data summary, and extension contributions
//! surfaced by the address bar identity button and Site Data Panel.

use crate::contributions::{ContributionId, ContributionSource};

/// Connection and origin security classification for a tab's current URL.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SiteSecurityState {
    Secure,
    #[default]
    NotSecure,
    Internal,
    File,
    Local,
    Extension,
}

impl SiteSecurityState {
    /// Human-readable security badge label shown in the site panel footer.
    pub fn label(self) -> &'static str {
        match self {
            Self::Secure => "Secure",
            Self::NotSecure => "Not Secure",
            Self::Internal => "Internal Page",
            Self::File => "Local File",
            Self::Local => "Local Page",
            Self::Extension => "Extension Page",
        }
    }

    /// Icon name in Zendia's icon table representing this security state.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Secure => "security-fill",
            Self::NotSecure => "security-broken-fill",
            Self::Internal => "sparkles",
            Self::File => "page-portrait",
            Self::Local => "globe",
            Self::Extension => "extension",
        }
    }

    /// Toolbar identity icon name shown inside the location bar.
    pub fn toolbar_icon(self) -> &'static str {
        match self {
            Self::Secure => "security",
            Self::NotSecure => "security-broken",
            Self::Internal => "sparkles",
            Self::File => "page-portrait",
            Self::Local => "globe",
            Self::Extension => "extension",
        }
    }

    /// Whether this security state should render in warning/danger red.
    pub fn is_warning(self) -> bool {
        matches!(self, Self::NotSecure)
    }

    /// Whether HTTP/HTTPS site controls (tracking protection, sharing, cookies) apply to this page.
    pub fn is_web_page(self) -> bool {
        matches!(self, Self::Secure | Self::NotSecure)
    }
}

/// Site permission categories manageable from the site data panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SitePermissionKind {
    Camera,
    Microphone,
    Location,
    Notifications,
    Autoplay,
    Popups,
    ScreenShare,
    Clipboard,
}

impl SitePermissionKind {
    pub const ALL: &'static [Self] = &[
        Self::Camera,
        Self::Microphone,
        Self::Location,
        Self::Notifications,
        Self::Autoplay,
        Self::Popups,
        Self::ScreenShare,
        Self::Clipboard,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Camera => "camera",
            Self::Microphone => "microphone",
            Self::Location => "location",
            Self::Notifications => "notifications",
            Self::Autoplay => "autoplay",
            Self::Popups => "popups",
            Self::ScreenShare => "screen_share",
            Self::Clipboard => "clipboard",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "camera" => Some(Self::Camera),
            "microphone" | "mic" => Some(Self::Microphone),
            "location" | "geo" | "geolocation" => Some(Self::Location),
            "notifications" | "notification" => Some(Self::Notifications),
            "autoplay" | "autoplay-media" => Some(Self::Autoplay),
            "popups" | "popup" => Some(Self::Popups),
            "screen_share" | "screen" => Some(Self::ScreenShare),
            "clipboard" => Some(Self::Clipboard),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Camera => "Camera",
            Self::Microphone => "Microphone",
            Self::Location => "Location",
            Self::Notifications => "Notifications",
            Self::Autoplay => "Autoplay",
            Self::Popups => "Pop-up Windows",
            Self::ScreenShare => "Screen Sharing",
            Self::Clipboard => "Clipboard",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Camera => "camera",
            Self::Microphone => "microphone",
            Self::Location => "globe",
            Self::Notifications => "bell",
            Self::Autoplay => "media-play",
            Self::Popups => "popup",
            Self::ScreenShare => "screen",
            Self::Clipboard => "edit-paste",
        }
    }
}

/// Effective permission grant state for one [`SitePermissionKind`] on a site origin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PermissionState {
    Allow,
    Block,
    #[default]
    Prompt,
}

impl PermissionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Block => "block",
            Self::Prompt => "prompt",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "allow" | "allowed" => Some(Self::Allow),
            "block" | "blocked" => Some(Self::Block),
            "prompt" | "ask" | "default" => Some(Self::Prompt),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Allow => "Allowed",
            Self::Block => "Blocked",
            Self::Prompt => "Ask",
        }
    }

    /// Toggles between `Allow` and `Block` (matching Zen's single-click permission toggle).
    pub fn toggled(self) -> Self {
        match self {
            Self::Allow => Self::Block,
            Self::Block | Self::Prompt => Self::Allow,
        }
    }
}

/// One permission entry associated with a site origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SitePermission {
    pub kind: SitePermissionKind,
    pub state: PermissionState,
}

/// Cookie and storage usage summary for a site origin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SiteDataSummary {
    pub cookie_count: u32,
    pub storage_bytes: u64,
}

impl SiteDataSummary {
    pub fn is_empty(self) -> bool {
        self.cookie_count == 0 && self.storage_bytes == 0
    }

    /// Formats a concise human-readable subtitle such as `"4 cookies · 128 KB"`.
    pub fn format_summary(self) -> String {
        if self.is_empty() {
            return "No stored data".to_string();
        }
        let cookie_word = if self.cookie_count == 1 { "cookie" } else { "cookies" };
        if self.storage_bytes == 0 {
            return format!("{} {cookie_word}", self.cookie_count);
        }
        let size = if self.storage_bytes >= 1_048_576 {
            format!("{:.1} MB", self.storage_bytes as f64 / 1_048_576.0)
        } else if self.storage_bytes >= 1024 {
            format!("{} KB", (self.storage_bytes + 511) / 1024)
        } else {
            format!("{} B", self.storage_bytes)
        };
        if self.cookie_count == 0 {
            size
        } else {
            format!("{} {cookie_word} · {size}", self.cookie_count)
        }
    }
}

/// Extension action contributed by ExtensionServer for the site data panel's Extensions grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteExtension {
    pub id: ContributionId,
    pub name: String,
    pub icon: String,
    pub enabled: bool,
    pub has_site_access: bool,
    pub command: String,
    pub source: ContributionSource,
}

/// Security, permissions, tracking protection, and site data state for a single site origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteInfo {
    pub origin: String,
    pub host: String,
    pub security: SiteSecurityState,
    pub tracking_protection_enabled: bool,
    pub permissions: Vec<SitePermission>,
    pub data: SiteDataSummary,
    pub bookmarked: bool,
}

impl SiteInfo {
    /// Constructs default site info derived from a tab URL.
    pub fn for_url(url: &str) -> Self {
        let security = security_state_for_url(url);
        let origin = origin_for_url(url);
        let host = host_for_url(url);
        Self {
            origin,
            host,
            security,
            tracking_protection_enabled: security.is_web_page(),
            permissions: Vec::new(),
            data: SiteDataSummary::default(),
            bookmarked: false,
        }
    }

    /// Permissions with explicit Allow or Block overrides (Prompt permissions stay hidden in the panel).
    pub fn visible_permissions(&self) -> Vec<&SitePermission> {
        self.permissions
            .iter()
            .filter(|perm| perm.state != PermissionState::Prompt)
            .collect()
    }

    /// Returns the effective state for `kind` (`Prompt` when unset).
    pub fn permission_state(&self, kind: SitePermissionKind) -> PermissionState {
        self.permissions
            .iter()
            .find(|perm| perm.kind == kind)
            .map(|perm| perm.state)
            .unwrap_or(PermissionState::Prompt)
    }

    /// Updates or inserts `kind` to `state`. Setting `Prompt` removes the explicit override.
    pub fn set_permission(&mut self, kind: SitePermissionKind, state: PermissionState) {
        if let Some(existing) = self.permissions.iter_mut().find(|perm| perm.kind == kind) {
            if state == PermissionState::Prompt {
                self.permissions.retain(|perm| perm.kind != kind);
            } else {
                existing.state = state;
            }
        } else if state != PermissionState::Prompt {
            self.permissions.push(SitePermission { kind, state });
        }
    }

    /// Clears cookies and local storage counts for this origin.
    pub fn clear_data(&mut self) {
        self.data = SiteDataSummary::default();
    }
}

/// Classifies the [`SiteSecurityState`] of a URL string.
pub fn security_state_for_url(url: &str) -> SiteSecurityState {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return SiteSecurityState::Internal;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("https://") {
        SiteSecurityState::Secure
    } else if lower.starts_with("http://") {
        SiteSecurityState::NotSecure
    } else if lower.starts_with("about:") || lower.starts_with("ladybird:") {
        SiteSecurityState::Internal
    } else if lower.starts_with("file://") {
        SiteSecurityState::File
    } else if lower.starts_with("chrome-extension://") || lower.starts_with("moz-extension://") {
        SiteSecurityState::Extension
    } else if lower.starts_with("local:") || lower.starts_with("plugin://") {
        SiteSecurityState::Local
    } else {
        SiteSecurityState::NotSecure
    }
}

/// Extracts a normalized origin key (such as `https://zendia.local` or `http://127.0.0.1:8080`) for `url`.
pub fn origin_for_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return "about:blank".to_string();
    }
    if let Some((scheme, rest)) = trimmed.split_once("://") {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        if authority.is_empty() {
            return format!("{}://", scheme.to_ascii_lowercase());
        }
        return format!("{}://{}", scheme.to_ascii_lowercase(), authority.to_ascii_lowercase());
    }
    if let Some((scheme, path)) = trimmed.split_once(':') {
        let page = path.split(['?', '#']).next().unwrap_or("");
        return format!("{}:{}", scheme.to_ascii_lowercase(), page);
    }
    trimmed.to_ascii_lowercase()
}

/// Extracts the display host (or internal page identifier) from `url`.
pub fn host_for_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return "about:blank".to_string();
    }
    if let Some((_, rest)) = trimmed.split_once("://") {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        let without_user = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
        if !without_user.is_empty() {
            return without_user.to_string();
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_security_states_across_schemes() {
        assert_eq!(
            security_state_for_url("https://zendia.local/docs"),
            SiteSecurityState::Secure
        );
        assert_eq!(
            security_state_for_url("http://127.0.0.1:8080/index.html"),
            SiteSecurityState::NotSecure
        );
        assert_eq!(security_state_for_url("about:newtab"), SiteSecurityState::Internal);
        assert_eq!(security_state_for_url("about:blank"), SiteSecurityState::Internal);
        assert_eq!(security_state_for_url("file:///etc/hosts"), SiteSecurityState::File);
        assert_eq!(
            security_state_for_url("chrome-extension://abc/popup.html"),
            SiteSecurityState::Extension
        );
        assert_eq!(security_state_for_url("plugin://demo/panel"), SiteSecurityState::Local);
        assert!(SiteSecurityState::NotSecure.is_warning());
        assert!(!SiteSecurityState::Secure.is_warning());
    }

    #[test]
    fn extracts_origin_and_host_cleanly() {
        assert_eq!(
            origin_for_url("HTTPS://Zendia.Local/docs?q=1#top"),
            "https://zendia.local"
        );
        assert_eq!(
            origin_for_url("http://127.0.0.1:8080/page.html"),
            "http://127.0.0.1:8080"
        );
        assert_eq!(origin_for_url("about:settings#general"), "about:settings");
        assert_eq!(host_for_url("https://zendia.local/docs"), "zendia.local");
        assert_eq!(host_for_url("http://127.0.0.1:8080/"), "127.0.0.1:8080");
    }

    #[test]
    fn updates_permissions_and_formats_site_data_summary() {
        let mut info = SiteInfo::for_url("https://zendia.local/docs");
        assert!(info.tracking_protection_enabled);
        assert!(info.visible_permissions().is_empty());

        info.set_permission(SitePermissionKind::Camera, PermissionState::Allow);
        info.set_permission(SitePermissionKind::Location, PermissionState::Block);
        assert_eq!(
            info.permission_state(SitePermissionKind::Camera),
            PermissionState::Allow
        );
        assert_eq!(info.visible_permissions().len(), 2);

        info.set_permission(SitePermissionKind::Camera, PermissionState::Prompt);
        assert_eq!(
            info.permission_state(SitePermissionKind::Camera),
            PermissionState::Prompt
        );
        assert_eq!(info.visible_permissions().len(), 1);

        info.data = SiteDataSummary {
            cookie_count: 3,
            storage_bytes: 24 * 1024,
        };
        assert_eq!(info.data.format_summary(), "3 cookies · 24 KB");
        info.clear_data();
        assert_eq!(info.data.format_summary(), "No stored data");
    }
}
