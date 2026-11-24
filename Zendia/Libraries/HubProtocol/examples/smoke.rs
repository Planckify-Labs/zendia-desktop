/*
 * Copyright (c) 2026-present, the Ladybird developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Starts the Hub, opens one view without any window, loads a page in it, and writes the first frame the Hub presents
//! to a PPM image. This checks the Rust side of the Hub protocol against the real Hub.
//!
//!     cargo run -p hub-protocol --example smoke -- <url> <output.ppm>

use hub_protocol::ipc_types::hub_client::messages as client;
use hub_protocol::ipc_types::hub_server::messages as server;
use hub_protocol::*;
use libipc_rust::wire::Utf8String;
use std::collections::HashMap;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn default_ui_info() -> Hub_UIInfo {
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
        preferred_languages: vec![Utf8String::from("en-US")],
        presentation_gpu_device: None,
        presentation_drm_format_modifiers: Vec::new(),
    }
}

#[allow(
    clippy::disallowed_methods,
    reason = "a successful mmap() of a non-empty length is never null"
)]
fn write_ppm(path: &str, bitmap: &SharedBitmap) -> std::io::Result<()> {
    let length = bitmap.size_in_bytes();
    // SAFETY: The Hub gave us a descriptor of shared memory at least this large, which we only read.
    let pixels = unsafe {
        let address = libc::mmap(
            std::ptr::null_mut(),
            length,
            libc::PROT_READ,
            libc::MAP_SHARED,
            bitmap.fd.as_raw_fd(),
            0,
        );
        if address == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error());
        }
        let pixels = std::slice::from_raw_parts(address as *const u8, length).to_vec();
        libc::munmap(address, length);
        pixels
    };
    let (width, height) = (bitmap.size.width as usize, bitmap.size.height as usize);
    let mut file = std::fs::File::create(path)?;
    write!(file, "P6\n{width} {height}\n255\n")?;
    let bgr = matches!(bitmap.format, bitmap_format::BGRX8888 | bitmap_format::BGRA8888);
    let mut rgb = Vec::with_capacity(width * height * 3);
    for pixel in pixels.as_chunks::<4>().0 {
        if bgr {
            rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        } else {
            rgb.extend_from_slice(&[pixel[0], pixel[1], pixel[2]]);
        }
    }
    file.write_all(&rgb)
}

fn add_backing_stores(
    backing_stores: &mut HashMap<i32, SharedBitmap>,
    bitmap_ids: Vec<i32>,
    images: Vec<Gfx_SharedImage>,
) {
    for (id, image) in bitmap_ids.into_iter().zip(images) {
        match image {
            Gfx_SharedImage::ShareableBitmap(Gfx_ShareableBitmap(Some(bitmap))) => {
                eprintln!("smoke: Backing store {id}: {:?}, format {}", bitmap.size, bitmap.format);
                backing_stores.insert(id, bitmap);
            }
            Gfx_SharedImage::LinuxDmaBuf(_) => eprintln!("smoke: Backing store {id} is a dma-buf"),
            other => eprintln!("smoke: Backing store {id} is not in CPU memory: {other:?}"),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let location = arguments.next().unwrap_or_else(|| "about:blank".to_string());
    let output = arguments.next().unwrap_or_else(|| "frame.ppm".to_string());

    let hub = find_hub_executable()?;
    eprintln!("smoke: Starting {}", hub.display());
    let (mut child, socket) = spawn_hub(&hub, &["--temporary-profile".to_string()], default_ui_info())?;

    let (sender, receiver) = mpsc::channel();
    let connection = HubConnection::new(socket, move |incoming| {
        let _ = sender.send(incoming);
    })?;

    let state = connection.send_sync(server::Initialize)?.state;
    eprintln!(
        "smoke: Initialized. User agent {:?}, config in {:?}",
        state.user_agent.as_str(),
        String::from_utf8_lossy(&state.config_directory.0)
    );

    let created = connection.send_sync(server::CreateView {
        request: Hub_NewViewRequest {
            is_private: WebView_IsPrivate::No,
            device_pixel_ratio: 1.0,
            maximum_frames_per_second: 60.0,
            display_id: None,
            screen_shows_high_dynamic_range: false,
            system_theme: Core_AnonymousBuffer::default(),
            screen_rects: None,
            owner_view_id: None,
            websites_prefer_dark: false,
            is_tab: true,
        },
    })?;
    let view_id = created.view_id;
    eprintln!("smoke: Created view {view_id} at {:?}", created.state.url.as_str());

    connection.post(server::ViewSetPresentsFromCpuMemory {
        view_id,
        presents_from_cpu_memory: true,
    })?;
    connection.post(server::ViewSetHostsRemoteLayer {
        view_id,
        hosts_remote_layer: false,
    })?;
    connection.post(server::ViewSetViewport {
        view_id,
        size: Web_DevicePixelSize {
            width: 1024,
            height: 768,
        },
        device_pixel_ratio: 1.0,
    })?;
    connection.post(server::ViewSetSystemVisibilityState {
        view_id,
        visibility_state: Web_HTML_VisibilityState::Visible,
    })?;
    connection.post(server::ViewSetCurrentTabOfWindow {
        view_id,
        current_tab_of_window: true,
    })?;
    connection.post(server::ViewSetHasSystemFocus {
        view_id,
        has_system_focus: true,
    })?;
    connection.post(server::ActiveViewChanged { view_id: Some(view_id) })?;
    connection.post(server::UiDidStart)?;

    let url = location::url_for_location(&location, location::DEFAULT_SEARCH_URL).ok_or("not a location")?;
    eprintln!("smoke: Loading {}", url.as_str());
    connection.post(server::ViewLoad {
        view_id,
        url,
        history_behavior: Web_Bindings_NavigationHistoryBehavior::Auto,
    })?;

    let mut backing_stores: HashMap<i32, SharedBitmap> = HashMap::new();
    let mut front: Option<i32> = None;
    let mut displayed: Option<i32> = None;
    let mut loaded_at: Option<Instant> = None;
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        // NB: A page that is done changing presents no more frames, so the frame on display a moment after the load
        //     finishes is the one to keep.
        if loaded_at.is_some_and(|at| at.elapsed() > Duration::from_millis(1500)) {
            let bitmap = displayed
                .and_then(|id| backing_stores.get(&id))
                .ok_or("no frame was presented")?;
            write_ppm(&output, bitmap)?;
            eprintln!("smoke: Wrote {output}");
            break;
        }
        let incoming = match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(incoming) => incoming,
            Err(_) => continue,
        };
        let message = match incoming {
            Incoming::Message(message) => *message,
            Incoming::Disconnected => return Err("the Hub went away".into()),
        };
        match message {
            HubMessage::ViewInstallBackingStores(client::ViewInstallBackingStores {
                bitmap_ids,
                backing_stores: images,
                ..
            }) => {
                backing_stores.clear();
                front = bitmap_ids.first().copied();
                displayed = None;
                add_backing_stores(&mut backing_stores, bitmap_ids, images);
            }
            HubMessage::ViewAddBackingStores(client::ViewAddBackingStores {
                bitmap_ids,
                backing_stores: images,
                ..
            }) => {
                add_backing_stores(&mut backing_stores, bitmap_ids, images);
            }
            HubMessage::ViewPresentBackingStore(present) => {
                eprintln!("smoke: Presented {} at {:?}", present.bitmap_id, present.size);
                displayed = Some(present.bitmap_id);
                // NB: The frame on display stays ours until the next one replaces it.
                if let Some(previous) = front.replace(present.bitmap_id) {
                    connection.post(server::ReleaseBackingStore {
                        page_id: present.page_id,
                        bitmap_id: previous,
                    })?;
                }
            }
            HubMessage::ViewLoadFinish(finish) => {
                eprintln!("smoke: Loaded {}", finish.url.as_str());
                if finish.url.as_str() != "about:blank" {
                    loaded_at = Some(Instant::now());
                }
            }
            HubMessage::ViewTitleChange(change) => eprintln!("smoke: Title {:?}", change.title.to_string_lossy()),
            _ => {}
        }
    }

    drop(connection);
    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}
