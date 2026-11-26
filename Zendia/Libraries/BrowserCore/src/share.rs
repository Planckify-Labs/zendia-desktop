/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Share link confirmation data model, payload construction, and stub share link generation.
//!
//! Covers sharing the current page (and spaces/folders), persisting the optional sharer display name and
//! "Don't ask me again" preference, and formatting the stub share link until a real share service is wired up.

use crate::commands::{ArgumentSchema, ArgumentType, Command, CommandDescriptor, CommandError};
use crate::model::{BrowserModel, FolderId, SpaceId, TabId};

/// Persisted user preferences for Zendia's share confirmation flow.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShareSettings {
    /// Optional author name shown to recipients (empty means anonymous sharing).
    pub display_name: String,
    /// When true, future share actions skip the confirmation dialog.
    pub dont_ask_again: bool,
}

/// What browser entity is being shared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShareTarget {
    Page {
        tab_id: Option<TabId>,
        title: String,
        url: String,
    },
    Space {
        space_id: SpaceId,
        name: String,
        tab_count: usize,
    },
    Folder {
        folder_id: FolderId,
        name: String,
        tab_count: usize,
    },
}

/// Complete description passed to the share confirmation dialog view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharePayload {
    pub target: ShareTarget,
    pub title: String,
    pub url: String,
    /// Human-readable one-line summary of what will be shared.
    pub summary: String,
    /// Pre-filled optional author name from profile settings.
    pub initial_name: String,
    /// Initial state of the "Don't ask me again" checkbox.
    pub initial_dont_ask: bool,
}

/// Maximum character length accepted for the optional author name (matches Zen's `maxlength="200"`).
pub const SHARE_NAME_MAX_LEN: usize = 200;

/// Returns whether `url` is a web URL eligible for sharing (`http://` or `https://`).
pub fn is_shareable_url(url: &str) -> bool {
    let trimmed = url.trim();
    trimmed.starts_with("http://") || trimmed.starts_with("https://")
}

/// Builds a [`SharePayload`] for the requested tab, space, or folder (defaulting to the active tab).
pub fn build_share_payload(
    model: &BrowserModel,
    tab_id: Option<TabId>,
    space_id: Option<SpaceId>,
    folder_id: Option<FolderId>,
) -> SharePayload {
    if let Some(fid) = folder_id
        && let Some(folder) = model.folder(fid)
    {
        let tabs = model.folder_tabs(fid);
        let first_url = tabs
            .iter()
            .find(|t| is_shareable_url(&t.url))
            .map(|t| t.url.clone())
            .unwrap_or_default();
        return SharePayload {
            target: ShareTarget::Folder {
                folder_id: fid,
                name: folder.name.clone(),
                tab_count: tabs.len(),
            },
            title: folder.name.clone(),
            url: first_url,
            summary: format!("Folder \"{}\" ({} tabs)", folder.name, tabs.len()),
            initial_name: model.share.display_name.clone(),
            initial_dont_ask: model.share.dont_ask_again,
        };
    }

    if let Some(sid) = space_id
        && let Some(space) = model.space(sid)
    {
        let tab_count = model.normal_tabs_in_space(sid).len() + model.pinned_tabs_in_space(sid).len();
        let first_url = model
            .normal_tabs_in_space(sid)
            .into_iter()
            .chain(model.pinned_tabs_in_space(sid))
            .find(|t| is_shareable_url(&t.url))
            .map(|t| t.url.clone())
            .unwrap_or_default();
        return SharePayload {
            target: ShareTarget::Space {
                space_id: sid,
                name: space.name.clone(),
                tab_count,
            },
            title: space.name.clone(),
            url: first_url,
            summary: format!("Space \"{}\" ({} tabs)", space.name, tab_count),
            initial_name: model.share.display_name.clone(),
            initial_dont_ask: model.share.dont_ask_again,
        };
    }

    let tab = tab_id.and_then(|id| model.tab(id)).or_else(|| model.active_tab());
    match tab {
        Some(tab) => {
            let title = tab.display_title();
            let url = tab.url.clone();
            SharePayload {
                target: ShareTarget::Page {
                    tab_id: Some(tab.id),
                    title: title.clone(),
                    url: url.clone(),
                },
                summary: if url.is_empty() {
                    title.clone()
                } else {
                    format!("{title} — {url}")
                },
                title,
                url,
                initial_name: model.share.display_name.clone(),
                initial_dont_ask: model.share.dont_ask_again,
            }
        }
        None => SharePayload {
            target: ShareTarget::Page {
                tab_id: None,
                title: "Current Page".to_string(),
                url: "https://zendia.local/".to_string(),
            },
            title: "Current Page".to_string(),
            url: "https://zendia.local/".to_string(),
            summary: "Current Page — https://zendia.local/".to_string(),
            initial_name: model.share.display_name.clone(),
            initial_dont_ask: model.share.dont_ask_again,
        },
    }
}

/// Sanitizes an author display name (trims whitespace and caps at [`SHARE_NAME_MAX_LEN`] chars).
pub fn normalize_display_name(raw: &str) -> String {
    raw.trim().chars().take(SHARE_NAME_MAX_LEN).collect()
}

/// Produces a deterministic stub share URL for a confirmed share action.
pub fn stub_share_link(title: &str, url: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in title.bytes().chain(std::iter::once(b'|')).chain(url.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("https://share.zendia.app/s/{:08x}", (hash & 0xffff_ffff) as u32)
}

/// Kind of shared collection represented by an incoming share link preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharePreviewKind {
    Space,
    Folder,
}

/// One entry rendered inside an incoming share preview card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SharePreviewItem {
    Tab {
        title: String,
        url: String,
        icon: Option<String>,
    },
    Folder {
        name: String,
        icon: String,
        items: Vec<SharePreviewItem>,
    },
    SplitView {
        tabs: Vec<(String, String, Option<String>)>,
    },
}

/// Preview metadata for an incoming Zendia share link (`.zen-share-overlay`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharePreview {
    pub url: String,
    pub share_id: String,
    pub kind: SharePreviewKind,
    pub name: String,
    pub sharer_name: Option<String>,
    pub items: Vec<SharePreviewItem>,
}

/// Extracts the share token if `url` is a Zendia or Zen share link, otherwise returns `None`.
pub fn parse_share_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    for prefix in [
        "https://share.zendia.app/s/",
        "http://share.zendia.app/s/",
        "https://zen-browser.app/share/",
    ] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let token = rest.split(['/', '?', '#']).next().unwrap_or("").trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    None
}

/// Builds a deterministic stub preview for an incoming share URL until the share server backend is wired up.
pub fn sample_share_preview(url: &str) -> SharePreview {
    let share_id = parse_share_url(url).unwrap_or_else(|| "preview".to_string());
    SharePreview {
        url: url.to_string(),
        share_id,
        kind: SharePreviewKind::Space,
        name: "Design & Research".to_string(),
        sharer_name: Some("Alex".to_string()),
        items: vec![
            SharePreviewItem::Folder {
                name: "UI Spec".to_string(),
                icon: "folder".to_string(),
                items: vec![
                    SharePreviewItem::Tab {
                        title: "Zendia Documentation".to_string(),
                        url: "https://zendia.local/docs".to_string(),
                        icon: Some("book".to_string()),
                    },
                    SharePreviewItem::Tab {
                        title: "Design Tokens — Zendia".to_string(),
                        url: "https://zendia.local/design".to_string(),
                        icon: Some("flame".to_string()),
                    },
                ],
            },
            SharePreviewItem::SplitView {
                tabs: vec![
                    (
                        "Architecture — Zendia".to_string(),
                        "https://zendia.local/architecture".to_string(),
                        Some("code".to_string()),
                    ),
                    (
                        "Releases — Zendia".to_string(),
                        "https://zendia.local/releases".to_string(),
                        Some("rocket".to_string()),
                    ),
                ],
            },
            SharePreviewItem::Tab {
                title: "Repository — Zendia".to_string(),
                url: "https://zendia.local/code".to_string(),
                icon: Some("code".to_string()),
            },
        ],
    }
}

fn find_arg<'a>(args: &'a [(String, String)], key: &str) -> Option<&'a str> {
    args.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// Parses `share.*` commands from string name and key-value arguments.
pub fn parse_command(name: &str, args: &[(String, String)]) -> Result<Option<Command>, CommandError> {
    let cmd = match name {
        "share.open" => {
            let tab_id = match find_arg(args, "tab_id") {
                Some(raw) => Some(TabId(raw.parse::<u64>().map_err(|_| {
                    CommandError::InvalidArgument {
                        command: name.to_string(),
                        message: format!("expected integer `tab_id`, got `{raw}`"),
                    }
                })?)),
                None => None,
            };
            let space_id = match find_arg(args, "space_id") {
                Some(raw) => Some(SpaceId(raw.parse::<u64>().map_err(|_| {
                    CommandError::InvalidArgument {
                        command: name.to_string(),
                        message: format!("expected integer `space_id`, got `{raw}`"),
                    }
                })?)),
                None => None,
            };
            let folder_id = match find_arg(args, "folder_id") {
                Some(raw) => Some(FolderId(raw.parse::<u64>().map_err(|_| {
                    CommandError::InvalidArgument {
                        command: name.to_string(),
                        message: format!("expected integer `folder_id`, got `{raw}`"),
                    }
                })?)),
                None => None,
            };
            Command::ShareOpen {
                tab_id,
                space_id,
                folder_id,
            }
        }
        "share.confirm" => {
            let display_name = find_arg(args, "name").unwrap_or_default().to_string();
            let dont_ask_again = find_arg(args, "dont_ask_again")
                .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
                .unwrap_or(false);
            let url = find_arg(args, "url").map(str::to_string);
            let title = find_arg(args, "title").map(str::to_string);
            Command::ShareConfirm {
                display_name,
                dont_ask_again,
                url,
                title,
            }
        }
        "share.cancel" => Command::ShareCancel,
        _ => return Ok(None),
    };
    Ok(Some(cmd))
}

/// Command descriptors for `share.*` commands.
pub fn command_descriptors() -> Vec<CommandDescriptor> {
    vec![
        CommandDescriptor {
            name: "share.open".to_string(),
            title: "Share Link".to_string(),
            description: "Open the share link confirmation dialog for the current page, space, or folder".to_string(),
            arguments: vec![
                ArgumentSchema::new(
                    "tab_id",
                    ArgumentType::TabId,
                    false,
                    "Optional tab identifier (defaults to active tab)",
                ),
                ArgumentSchema::new(
                    "space_id",
                    ArgumentType::SpaceId,
                    false,
                    "Optional space identifier to share",
                ),
                ArgumentSchema::new(
                    "folder_id",
                    ArgumentType::FolderId,
                    false,
                    "Optional folder identifier to share",
                ),
            ],
        },
        CommandDescriptor {
            name: "share.confirm".to_string(),
            title: "Confirm Share Link".to_string(),
            description: "Confirm sharing and copy the generated share link".to_string(),
            arguments: vec![
                ArgumentSchema::new("name", ArgumentType::String, false, "Optional sharer display name"),
                ArgumentSchema::new(
                    "dont_ask_again",
                    ArgumentType::Bool,
                    false,
                    "Remember choice and skip future confirmations",
                ),
                ArgumentSchema::new(
                    "url",
                    ArgumentType::String,
                    false,
                    "URL being shared (defaults to active tab)",
                ),
                ArgumentSchema::new(
                    "title",
                    ArgumentType::String,
                    false,
                    "Title being shared (defaults to active tab)",
                ),
            ],
        },
        CommandDescriptor {
            name: "share.cancel".to_string(),
            title: "Cancel Share".to_string(),
            description: "Cancel and dismiss the share confirmation dialog".to_string(),
            arguments: vec![],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::sample_model;

    #[test]
    fn share_payload_targets_active_page_space_and_folder() {
        let model = sample_model(true);

        let page_payload = build_share_payload(&model, None, None, None);
        assert!(matches!(page_payload.target, ShareTarget::Page { .. }));
        assert!(!page_payload.title.is_empty());
        assert!(is_shareable_url(&page_payload.url));

        let space_payload = build_share_payload(&model, None, Some(SpaceId(1)), None);
        assert!(matches!(
            space_payload.target,
            ShareTarget::Space {
                space_id: SpaceId(1),
                ..
            }
        ));
        assert_eq!(space_payload.title, "Space");

        let folder_id = model.folders[0].id;
        let folder_payload = build_share_payload(&model, None, None, Some(folder_id));
        assert!(matches!(folder_payload.target, ShareTarget::Folder { .. }));
    }

    #[test]
    fn normalize_display_name_trims_and_clamps_length() {
        assert_eq!(normalize_display_name("  Alex  "), "Alex");
        let long = "a".repeat(250);
        assert_eq!(normalize_display_name(&long).len(), SHARE_NAME_MAX_LEN);
    }

    #[test]
    fn stub_share_link_is_deterministic_and_url_shaped() {
        let a = stub_share_link("Docs", "https://zendia.local/docs");
        let b = stub_share_link("Docs", "https://zendia.local/docs");
        let c = stub_share_link("Other", "https://zendia.local/other");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.starts_with("https://share.zendia.app/s/"));
    }

    #[test]
    fn shareable_url_accepts_http_and_https_only() {
        assert!(is_shareable_url("https://example.org/page"));
        assert!(is_shareable_url("http://127.0.0.1:8782/"));
        assert!(!is_shareable_url("about:blank"));
        assert!(!is_shareable_url("file:///tmp/a.html"));
    }

    #[test]
    fn stub_share_open_and_confirm_flow_updates_model_and_copies_link() {
        use crate::BrowserCore;
        use crate::events::{Event, ViewRequest};
        use crate::stub::StubBackend;

        let core = StubBackend::new();
        let sub = core.subscribe();
        let mut replica = core.model();

        core.invoke(Command::ShareOpen {
            tab_id: None,
            space_id: None,
            folder_id: None,
        })
        .unwrap();
        let ev = sub.try_recv().unwrap();
        assert!(matches!(
            ev,
            Event::ViewRequested {
                request: ViewRequest::ShareConfirmOpen { .. },
                ..
            }
        ));

        core.invoke(Command::ShareConfirm {
            display_name: "Alex".to_string(),
            dont_ask_again: true,
            url: Some("https://zendia.local/docs".to_string()),
            title: Some("Zendia Docs".to_string()),
        })
        .unwrap();

        let mut copied_link = None;
        let mut saw_close = false;
        while let Ok(ev) = sub.try_recv() {
            replica.apply_event(&ev);
            match ev {
                Event::ClipboardCopyRequested { text } => copied_link = Some(text),
                Event::ViewRequested {
                    request: ViewRequest::ShareConfirmClose,
                    ..
                } => saw_close = true,
                _ => {}
            }
        }
        assert!(copied_link.is_some_and(|l| l.starts_with("https://share.zendia.app/s/")));
        assert!(saw_close);
        assert_eq!(replica.share.display_name, "Alex");
        assert!(replica.share.dont_ask_again);
    }

    #[test]
    fn parse_share_url_recognizes_zendia_and_zen_share_links() {
        assert_eq!(
            parse_share_url("https://share.zendia.app/s/deadbeef?v=1"),
            Some("deadbeef".to_string())
        );
        assert_eq!(
            parse_share_url("https://zen-browser.app/share/abc123"),
            Some("abc123".to_string())
        );
        assert_eq!(parse_share_url("https://example.org/docs"), None);
        assert_eq!(parse_share_url("https://share.zendia.app/s/"), None);

        let preview = sample_share_preview("https://share.zendia.app/s/deadbeef");
        assert_eq!(preview.share_id, "deadbeef");
        assert_eq!(preview.items.len(), 3);
    }

    #[test]
    fn stub_share_open_rejects_internal_urls_with_toast() {
        use crate::BrowserCore;
        use crate::events::{Event, ViewRequest};
        use crate::stub::StubBackend;

        let core = StubBackend::new();
        let sub = core.subscribe();
        core.invoke(Command::TabNew {
            url: Some("about:blank".to_string()),
            after: None,
            activate: true,
        })
        .unwrap();
        while sub.try_recv().is_ok() {}

        core.invoke(Command::ShareOpen {
            tab_id: None,
            space_id: None,
            folder_id: None,
        })
        .unwrap();

        let mut saw_dialog = false;
        let mut saw_toast = false;
        while let Ok(ev) = sub.try_recv() {
            match ev {
                Event::ViewRequested {
                    request: ViewRequest::ShareConfirmOpen { .. },
                    ..
                } => saw_dialog = true,
                Event::ToastShown { toast } if toast.message.contains("cannot be shared") => saw_toast = true,
                _ => {}
            }
        }
        assert!(!saw_dialog);
        assert!(saw_toast);
    }
}
