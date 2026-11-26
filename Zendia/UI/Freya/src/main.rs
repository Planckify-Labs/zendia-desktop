/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Ladybird, with a UI written in Rust with Freya. Like the Qt UI, it is a process of its own, which starts the Hub and
//! shows the views that live there.

use freya::prelude::*;
use hub_protocol::ipc_types::hub_server::messages as server;
use hub_protocol::*;
use libipc_rust::wire::Utf8String;
use zendia::{browser, hub, settings};

fn system_locale() -> String {
    std::env::var("LANG")
        .ok()
        .and_then(|lang| lang.split('.').next().map(|tag| tag.replace('_', "-")))
        .filter(|tag| !tag.is_empty() && tag != "C" && tag != "POSIX")
        .unwrap_or_else(|| "en-US".to_string())
}

fn ui_info(locale: &str) -> Hub_UIInfo {
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
        preferred_languages: vec![Utf8String::from(locale)],
        presentation_gpu_device: None,
        presentation_drm_format_modifiers: Vec::new(),
    }
}

struct ExtensionCliArgs {
    load_extensions: Vec<String>,
    auto_accept_extension_prompts: bool,
    hub_arguments: Vec<String>,
}

fn parse_cli_args() -> ExtensionCliArgs {
    let mut load_extensions = Vec::new();
    let mut auto_accept_extension_prompts = false;
    let mut hub_arguments = Vec::new();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--load-extension" {
            if let Some(path) = args.next()
                && !path.is_empty()
            {
                load_extensions.push(path);
            }
        } else if let Some(path) = arg.strip_prefix("--load-extension=") {
            if !path.is_empty() {
                load_extensions.push(path.to_string());
            }
        } else if arg == "--auto-accept-extension-prompts" {
            auto_accept_extension_prompts = true;
        } else {
            hub_arguments.push(arg);
        }
    }

    ExtensionCliArgs {
        load_extensions,
        auto_accept_extension_prompts,
        hub_arguments,
    }
}

fn connect_to_hub() -> Result<Option<std::process::Child>, Box<dyn std::error::Error>> {
    let hub_path = find_hub_executable()?;
    let cli = parse_cli_args();
    let locale = system_locale();
    // NB: The Hub takes the same arguments as the Qt UI does, such as the URLs to open and the profile to use.
    let (mut child, socket) = spawn_hub(&hub_path, &cli.hub_arguments, ui_info(&locale))?;

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

    let data_directory = std::path::PathBuf::from(String::from_utf8_lossy(&state.data_directory.0).into_owned());
    hub::set_startup(hub::Startup {
        connection,
        incoming: receiver,
        settings: settings::Settings::from_json(&state.settings_json.0),
        initial_urls: state.urls,
        data_directory,
        locale,
        load_extensions: cli.load_extensions,
        auto_accept_extension_prompts: cli.auto_accept_extension_prompts,
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
