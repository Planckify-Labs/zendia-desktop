/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! What the UI shows about an extension install: the consent prompt's details and the install card's state.

use base64::Engine;
use serde_json::{Value, json};

/// Stops a running install or gives up a failed one. It gets the failure, or `None` when the user cancelled.
pub type CancelAction = Box<dyn FnOnce(Option<String>)>;
/// Starts a failed install again.
pub type RetryAction = Box<dyn FnOnce()>;

/// The largest store icon the browser fetches for a prompt.
pub const MAX_ICON_BYTES: usize = 512 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    Downloading,
    Checking,
    Installing,
    /// The package is read and the user is asked; the prompt replaces the card.
    AwaitingApproval,
}

impl Stage {
    fn as_str(self) -> &'static str {
        match self {
            Stage::Downloading => "downloading",
            Stage::Checking => "checking",
            Stage::Installing => "installing",
            Stage::AwaitingApproval => "awaiting_approval",
        }
    }
}

/// An install the UI shows a card for.
pub struct Card {
    pub extension_id: String,
    pub name: String,
    pub source: String,
    pub icon: Option<Icon>,
    pub failure: Option<String>,
    pub on_cancel: Option<CancelAction>,
    pub on_retry: Option<RetryAction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Icon {
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

impl Icon {
    pub fn to_json(&self) -> Value {
        json!({
            "kind": "encoded",
            "mime": self.mime,
            "data": base64::engine::general_purpose::STANDARD.encode(&self.bytes),
        })
    }
}

impl Card {
    fn base_json(&self, install_id: u64) -> Value {
        json!({
            "op": "install_progress",
            "installId": install_id,
            "extensionId": self.extension_id,
            "name": self.name,
            "source": self.source,
            "icon": self.icon.as_ref().map(Icon::to_json),
        })
    }

    pub fn stage_json(&self, install_id: u64, stage: Stage) -> Value {
        let mut v = self.base_json(install_id);
        v["stage"] = json!(stage.as_str());
        v
    }

    pub fn failed_json(&self, install_id: u64, reason: &str, can_retry: bool) -> Value {
        let mut v = self.base_json(install_id);
        v["stage"] = json!("failed");
        v["reason"] = json!(reason);
        v["canRetry"] = json!(can_retry);
        v
    }

    pub fn installed_json(&self, install_id: u64, extension_id: &str, updated: bool) -> Value {
        let mut v = self.base_json(install_id);
        v["stage"] = json!("installed");
        v["extensionId"] = json!(extension_id);
        v["updated"] = json!(updated);
        v
    }
}

pub fn cancelled_json(install_id: u64) -> Value {
    json!({ "op": "install_progress", "installId": install_id, "stage": "cancelled" })
}

/// A permission warning as a prompt detail. Broad access is marked so the prompt can make it stand out.
pub fn warning_detail(text: &str) -> Value {
    json!({ "text": text, "highRisk": is_high_risk_warning(text) })
}

pub fn plain_details(texts: Vec<String>) -> Vec<Value> {
    texts
        .into_iter()
        .map(|text| json!({ "text": text, "highRisk": false }))
        .collect()
}

/// The warnings `webext::store::permission_warnings` writes for access to every site, history, the debugger, native
/// programs, the screen, the clipboard and the browser's own settings.
fn is_high_risk_warning(text: &str) -> bool {
    const HIGH_RISK: &[&str] = &[
        "all your data on all websites",
        "browsing history",
        "page debugger",
        "native applications",
        "Capture content of your screen",
        "privacy-related settings",
        "Manage your apps, extensions, and themes",
        "data you copy and paste",
        "control websites' access",
    ];
    HIGH_RISK.iter().any(|needle| text.contains(needle))
}

/// Recognises the image formats Freya can show from their first bytes. The store's own content type is not trusted.
pub fn sniff_icon(bytes: &[u8]) -> Option<Icon> {
    if bytes.is_empty() || bytes.len() > MAX_ICON_BYTES {
        return None;
    }
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
        if head.contains("<svg") {
            "image/svg+xml"
        } else {
            return None;
        }
    };
    Some(Icon {
        mime,
        bytes: bytes.to_vec(),
    })
}

/// Decodes a `data:` URL's bytes (base64 or percent-encoded).
pub fn data_url_bytes(url: &str) -> Option<Vec<u8>> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    if meta.ends_with(";base64") {
        base64::engine::general_purpose::STANDARD.decode(data.trim()).ok()
    } else {
        Some(percent_decode(data))
    }
}

fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_broad_access_as_high_risk() {
        assert_eq!(
            warning_detail("Read and change all your data on all websites")["highRisk"],
            true
        );
        assert_eq!(warning_detail("Read your browsing history")["highRisk"], true);
        assert_eq!(warning_detail("Display notifications")["highRisk"], false);
        assert_eq!(
            warning_detail("Read and change your data on example.com")["highRisk"],
            false
        );
    }

    #[test]
    fn sniffs_icon_formats() {
        assert_eq!(sniff_icon(b"\x89PNG\r\n\x1a\nrest").unwrap().mime, "image/png");
        assert_eq!(
            sniff_icon(b"<?xml version=\"1.0\"?><svg xmlns=\"\"/>").unwrap().mime,
            "image/svg+xml"
        );
        assert!(sniff_icon(b"<html>not an image</html>").is_none());
        assert!(sniff_icon(&[]).is_none());
    }

    #[test]
    fn decodes_data_urls() {
        assert_eq!(
            data_url_bytes("data:image/png;base64,iVBORw==").unwrap(),
            vec![0x89, 0x50, 0x4E, 0x47]
        );
        assert_eq!(
            data_url_bytes("data:image/svg+xml,%3Csvg%3E").unwrap(),
            b"<svg>".to_vec()
        );
        assert!(data_url_bytes("https://example.com/icon.png").is_none());
    }
}
