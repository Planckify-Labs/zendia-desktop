/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Native renderer for WebExtension icons (`Encoded` SVG/PNG/JPEG/WebP, raw `Rgba` bitmaps, or fallback kit icon).

use crate::ui::kit::Icon;
use crate::ui::theme::ThemeRole;
use freya::elements::image::{ImageHandle, image};
use freya::engine::prelude::{AlphaType, ColorType, ISize, Image as SkImage, ImageInfo, SkData, raster_from_data};
use freya::prelude::*;
use zendia_browser_core::modules::ExtensionIcon;

/// Renders a WebExtension's real icon payload when available, falling back cleanly to a named kit [`Icon`].
#[derive(Clone, PartialEq)]
pub struct ExtensionIconView {
    pub cache_key: String,
    pub fallback_icon: String,
    pub icon_data: Option<ExtensionIcon>,
    pub size: f32,
    pub fallback_role: ThemeRole,
}

impl ExtensionIconView {
    pub fn new(
        cache_key: impl Into<String>,
        fallback_icon: impl Into<String>,
        icon_data: Option<ExtensionIcon>,
        size: f32,
        fallback_role: ThemeRole,
    ) -> Self {
        Self {
            cache_key: cache_key.into(),
            fallback_icon: fallback_icon.into(),
            icon_data,
            size,
            fallback_role,
        }
    }
}

fn decode_raster_handle(icon: &ExtensionIcon) -> Option<ImageHandle> {
    match icon {
        ExtensionIcon::Encoded { mime, bytes } => {
            if mime.eq_ignore_ascii_case("image/svg+xml") || bytes.is_empty() {
                return None;
            }
            let owned = Bytes::copy_from_slice(bytes);
            // SAFETY: `ImageHandle` keeps `owned` alive for as long as the Skia image exists.
            let data = unsafe { SkData::new_bytes(&owned) };
            let img = SkImage::from_encoded(data)?;
            Some(ImageHandle::new(img, owned))
        }
        ExtensionIcon::Rgba { width, height, bytes } => {
            let expected = (*width as usize).checked_mul(*height as usize)?.checked_mul(4)?;
            if *width == 0 || *height == 0 || bytes.len() < expected {
                return None;
            }
            let owned = Bytes::copy_from_slice(&bytes[..expected]);
            let info = ImageInfo::new(
                ISize::new(*width as i32, *height as i32),
                ColorType::RGBA8888,
                AlphaType::Unpremul,
                None,
            );
            // SAFETY: `ImageHandle` keeps `owned` alive for as long as the Skia image exists.
            let data = unsafe { SkData::new_bytes(&owned) };
            let img = raster_from_data(&info, data, (*width as usize) * 4)?;
            Some(ImageHandle::new(img, owned))
        }
    }
}

impl Component for ExtensionIconView {
    fn render(&self) -> impl IntoElement {
        if let Some(icon_data) = &self.icon_data {
            if let ExtensionIcon::Encoded { mime, bytes } = icon_data
                && mime.eq_ignore_ascii_case("image/svg+xml")
                && !bytes.is_empty()
            {
                return SvgViewer::new((("ext-svg", self.cache_key.clone()), Bytes::copy_from_slice(bytes)))
                    .width(Size::px(self.size))
                    .height(Size::px(self.size))
                    .into_element();
            }
            if let Some(handle) = decode_raster_handle(icon_data) {
                return image(handle)
                    .width(Size::px(self.size))
                    .height(Size::px(self.size))
                    .into_element();
            }
        }

        Icon::new(self.fallback_icon.clone())
            .size(self.size)
            .role(self.fallback_role)
            .into_element()
    }
}
