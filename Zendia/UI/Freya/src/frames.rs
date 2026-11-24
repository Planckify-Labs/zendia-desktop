/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The backing stores the Compositor presents a page's frames in, as the UI sees them, and the images it shows.
//!
//! The Hub shares each backing store once, and then names the one that holds each new frame. The UI copies that frame
//! into an image of its own, so it never draws from memory the Compositor may paint into. The backing store on display
//! stays the UI's until the next frame replaces it, and then goes back to the Compositor, as `WebView::PageBackingStores`
//! has it.

use freya::elements::image::ImageHandle;
use freya::engine::prelude::{AlphaType, ColorType, ISize, ImageInfo, SkData, raster_from_data};
use freya::prelude::Bytes;
use hub_protocol::{Gfx_IntSize, Gfx_ShareableBitmap, Gfx_SharedImage, SharedBitmap, alpha_type, bitmap_format};
use std::collections::{HashMap, HashSet};
use std::os::fd::AsRawFd;

/// Shared memory, mapped for reading.
struct Mapping {
    address: *mut libc::c_void,
    length: usize,
}

impl Mapping {
    fn new(bitmap: &SharedBitmap) -> std::io::Result<Self> {
        let length = bitmap.size_in_bytes();
        if length == 0 {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        // SAFETY: This maps a descriptor of shared memory that we own, read-only, and keeps it until the mapping drops.
        let address = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ,
                libc::MAP_SHARED,
                bitmap.fd.as_raw_fd(),
                0,
            )
        };
        if address == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { address, length })
    }

    #[allow(
        clippy::disallowed_methods,
        reason = "a successful mmap() of a non-empty length is never null"
    )]
    fn bytes(&self) -> &[u8] {
        // SAFETY: The mapping is `length` bytes long and lives as long as `self`.
        unsafe { std::slice::from_raw_parts(self.address as *const u8, self.length) }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: This unmaps what `new()` mapped, once.
        unsafe {
            libc::munmap(self.address, self.length);
        }
    }
}

struct BackingStore {
    mapping: Mapping,
    size: Gfx_IntSize,
    format: u32,
    alpha_type: u32,
}

impl BackingStore {
    fn new(bitmap: SharedBitmap) -> std::io::Result<Self> {
        Ok(Self {
            mapping: Mapping::new(&bitmap)?,
            size: bitmap.size,
            format: bitmap.format,
            alpha_type: bitmap.alpha_type,
        })
    }

    /// Copies the part of the backing store that holds a frame of this size.
    fn copy_frame(&self, size: Gfx_IntSize) -> Option<ImageHandle> {
        let width = size.width.clamp(0, self.size.width);
        let height = size.height.clamp(0, self.size.height);
        if width == 0 || height == 0 {
            return None;
        }
        let pitch = self.size.width as usize * 4;
        let bytes = Bytes::copy_from_slice(&self.mapping.bytes()[..pitch * height as usize]);

        let color_type = match self.format {
            bitmap_format::BGRX8888 | bitmap_format::BGRA8888 => ColorType::BGRA8888,
            _ => ColorType::RGBA8888,
        };
        let alpha = match (self.format, self.alpha_type) {
            (bitmap_format::BGRX8888 | bitmap_format::RGBX8888, _) => AlphaType::Opaque,
            (_, alpha_type::PREMULTIPLIED) => AlphaType::Premul,
            _ => AlphaType::Unpremul,
        };
        let info = ImageInfo::new(ISize::new(width, height), color_type, alpha, None);
        // SAFETY: The image handle owns `bytes`, which outlives the image made from them.
        let data = unsafe { SkData::new_bytes(&bytes) };
        let image = raster_from_data(&info, data, pitch)?;
        Some(ImageHandle::new(image, bytes))
    }
}

/// What the UI does with a frame the Hub presents.
pub struct Presentation {
    /// The frame to show now, unless it is in memory the UI cannot read.
    pub frame: Option<ImageHandle>,
    /// The backing store that goes back to the Compositor.
    pub release: Option<i32>,
}

#[derive(Default)]
pub struct PageBackingStores {
    page_id: u64,
    front: Option<i32>,
    stores: HashMap<i32, BackingStore>,
    // NB: Backing stores in GPU memory, which this UI does not show. The Hub presents from those in CPU memory once
    //     the UI asks it to, but it still shares both.
    other_ids: HashSet<i32>,
}

impl PageBackingStores {
    pub fn page_id(&self) -> u64 {
        self.page_id
    }

    fn take(&mut self, ids: Vec<i32>, images: Vec<Gfx_SharedImage>) {
        for (id, image) in ids.into_iter().zip(images) {
            match image {
                Gfx_SharedImage::ShareableBitmap(Gfx_ShareableBitmap(Some(bitmap))) => {
                    match BackingStore::new(bitmap) {
                        Ok(store) => {
                            self.stores.insert(id, store);
                        }
                        Err(error) => {
                            eprintln!("zendia: Unable to map backing store {id}: {error}");
                            self.other_ids.insert(id);
                        }
                    }
                }
                _ => {
                    self.other_ids.insert(id);
                }
            }
        }
    }

    /// Replaces every backing store. The first of the new ones becomes the front backing store.
    pub fn install(&mut self, page_id: u64, ids: Vec<i32>, images: Vec<Gfx_SharedImage>) {
        self.page_id = page_id;
        self.front = ids.first().copied();
        self.stores.clear();
        self.other_ids.clear();
        self.take(ids, images);
    }

    pub fn add(&mut self, ids: Vec<i32>, images: Vec<Gfx_SharedImage>) {
        self.take(ids, images);
    }

    pub fn retire(&mut self, ids: &[i32]) {
        for id in ids {
            if Some(*id) == self.front {
                continue;
            }
            self.stores.remove(id);
            self.other_ids.remove(id);
        }
    }

    pub fn clear(&mut self) {
        self.front = None;
        self.stores.clear();
        self.other_ids.clear();
    }

    pub fn present(&mut self, id: i32, size: Gfx_IntSize) -> Presentation {
        let is_ours = self.stores.contains_key(&id) || self.other_ids.contains(&id);
        if !is_ours || Some(id) == self.front {
            // NB: A frame in a backing store that is gone, or not one of ours, goes straight back.
            return Presentation {
                frame: None,
                release: if is_ours { None } else { Some(id) },
            };
        }
        let frame = self.stores.get(&id).and_then(|store| store.copy_frame(size));
        let release = self.front.replace(id);
        Presentation { frame, release }
    }
}
