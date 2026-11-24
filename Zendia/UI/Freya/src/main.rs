/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Ladybird, with a UI written in Rust with Freya. Like the Qt UI, it is a process of its own, which starts the Hub and
//! shows the views that live there.

mod browser;
mod frames;
mod hub;
mod input;
mod settings;

use freya::prelude::*;
use hub_protocol::ipc_types::hub_server::messages as server;
use hub_protocol::*;
use libipc_rust::wire::Utf8String;

fn ui_info() -> Hub_UIInfo {
    let language = std::env::var("LANG")
        .ok()
        .and_then(|lang| lang.split('.').next().map(|tag| tag.replace('_', "-")))
        .filter(|tag| !tag.is_empty() && tag != "C" && tag != "POSIX")
        .unwrap_or_else(|| "en-US".to_string());
    Hub_UIInfo {
        ui_font_family: None,
        system_font_family: None,
        interface_color_scheme: Web_CSS_PreferredColorScheme::Auto,
        supports_picture_in_picture: false,
        has_location_source: false,
        has_translator: false,
        has_spell_checker: false,
        spell_checker_learns_for_every_tag: false,
        spell_checker_can_suggest: false,
        microphone_authorization: None,
        location_authorization: None,
        supports_selection_clipboard: false,
        supports_system_menu_bar: false,
        supports_vertical_tabs: false,
        supports_private_browsing_windows: false,
        supports_client_side_window_decorations: false,
        reports_scroll_momentum: false,
        scrolls_elastically: false,
        scrollbar_track_press_jumps: false,
        prefers_reduced_motion: false,
        prefers_more_contrast: false,
        preferred_languages: vec![Utf8String::from(language)],
        presentation_gpu_device: None,
        presentation_drm_format_modifiers: Vec::new(),
    }
}

fn connect_to_hub() -> Result<Option<std::process::Child>, Box<dyn std::error::Error>> {
    let hub_path = find_hub_executable()?;
    // NB: The Hub takes the same arguments as the Qt UI does, such as the URLs to open and the profile to use.
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let (mut child, socket) = spawn_hub(&hub_path, &arguments, ui_info())?;

    let (sender, receiver) = async_channel::unbounded();
    let connection = HubConnection::new(socket, move |incoming| {
        let _ = sender.send_blocking(incoming);
    })?;

    // NB: The Hub can find that there is nothing for this UI to do, as when another browser has the profile and takes
    //     our URLs. It ends right away then.
    let state = match connection.send_sync(server::Initialize) {
        Ok(response) => response.state,
        Err(error) => {
            let status = child.wait()?;
            if status.success() {
                return Ok(None);
            }
            return Err(format!("the Hub did not start: {error} ({status})").into());
        }
    };

    hub::set_startup(hub::Startup {
        connection,
        incoming: receiver,
        settings: settings::Settings::from_json(&state.settings_json.0),
        initial_urls: state.urls,
    });
    Ok(Some(child))
}

fn main() {
    let mut hub_process = match connect_to_hub() {
        Ok(Some(process)) => process,
        Ok(None) => return,
        Err(error) => {
            eprintln!("zendia: {error}");
            std::process::exit(1);
        }
    };

    launch(
        LaunchConfig::new().with_window(
            WindowConfig::new(browser::app)
                .with_title("Ladybird")
                .with_size(1280., 860.),
        ),
    );

    // NB: The Hub ends by itself once its connection to the UI closes.
    let _ = hub_process.wait();
}
