/*
 * Copyright (c) 2026-present, the Ladybird developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The types the Hub's messages carry, under the names the IPC compiler gives them. Most are generated from their .ipc
//! declarations by generate.py. The ones below have no declaration, so their wire forms are written out here, each as
//! its C++ `IPC::encode()` and `IPC::decode()` have it.

#![allow(non_camel_case_types)]

use libipc_rust::wire::{Decode, DecodeError, Decoder, Encode, EncodeError, Encoder, File, Utf8String};
use std::os::fd::{AsFd, OwnedFd};
use std::sync::Arc;

// NB: These are AK::DistinctNumeric types, which go as the number they hold.
pub type Web_DevicePixels = i32;
pub type Web_UniqueNodeID = i64;
pub type Web_CompositorContextId = u64;

/// `UnixDateTime`: nanoseconds since the epoch.
pub type UnixDateTime = i64;

/// `Gfx::Color`: the color as 0xAARRGGBB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Encode, Decode)]
#[repr(transparent)]
pub struct Gfx_Color(pub u32);

impl Gfx_Color {
    pub const fn alpha(self) -> u8 {
        (self.0 >> 24) as u8
    }
    pub const fn red(self) -> u8 {
        (self.0 >> 16) as u8
    }
    pub const fn green(self) -> u8 {
        (self.0 >> 8) as u8
    }
    pub const fn blue(self) -> u8 {
        self.0 as u8
    }
}

macro_rules! wire_enum {
    ($(#[$meta:meta])* $name:ident : $repr:ty { $($variant:ident),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Encode, Decode)]
        #[repr($repr)]
        pub enum $name {
            $($variant),+
        }
    };
}

// NB: A C++ enum class declared without an underlying type has int as its type, and goes as one.
wire_enum!(Web_Bindings_NavigationHistoryBehavior: u8 { Auto, Push, Replace });
wire_enum!(Web_EventResult: i32 { Accepted, Handled, Dropped, Cancelled });
wire_enum!(Web_HTML_ActivateTab: i32 { Yes, No });
wire_enum!(Web_HTML_MuteState: i32 { Muted, Unmuted });
wire_enum!(Web_HTML_VisibilityState: u8 { Visible, Hidden });
wire_enum!(Web_NavigatorCompatibilityMode: i32 { Chrome, Gecko, WebKit });
wire_enum!(Web_Permissions_PermissionName: u8 { Geolocation, Microphone, Notifications });
wire_enum!(WebView_Application_ClipboardType: u8 { Text, Selection });
wire_enum!(WebView_BypassCache: u8 { No, Yes });
wire_enum!(WebView_ContentBlockingState: u8 { Unavailable, Disabled, SiteDisabled, Enabled, NoRules });
wire_enum!(WebView_FileDownloader_DownloadStatus: u8 { InProgress, Paused, Completed, Canceled, Failed });
wire_enum!(WebView_IsPrivate: u8 { No, Yes });
wire_enum!(WebView_PermissionDecision: u8 { Allow, AllowOnce, Block });
wire_enum!(Web_CSS_PreferredColorScheme: i32 { Auto, Dark, Light });
wire_enum!(WebView_MicrophoneAuthorization: u8 { NotDetermined, Authorized, Denied });
wire_enum!(Core_GeolocationAuthorization: u8 { NotDetermined, Authorized, Denied, Unknown });

/// `Gfx::GpuDeviceIdentity`: bytes that name a GPU, as the platform's graphics API gives them.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Gfx_GpuDeviceIdentity {
    pub bytes: Vec<u8>,
}

/// `Core::AnonymousBuffer`: shared memory, which goes as whether there is any, then its size and a descriptor of it.
#[derive(Clone, Debug, Default)]
pub struct Core_AnonymousBuffer {
    pub shared_memory: Option<(u64, Arc<OwnedFd>)>,
}

impl PartialEq for Core_AnonymousBuffer {
    fn eq(&self, other: &Self) -> bool {
        match (&self.shared_memory, &other.shared_memory) {
            (None, None) => true,
            (Some((a_size, a_fd)), Some((b_size, b_fd))) => a_size == b_size && Arc::ptr_eq(a_fd, b_fd),
            _ => false,
        }
    }
}

impl Encode for Core_AnonymousBuffer {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), EncodeError> {
        encoder.encode(&self.shared_memory.is_some())?;
        if let Some((size, fd)) = &self.shared_memory {
            encoder.encode(size)?;
            let fd = fd.as_fd().try_clone_to_owned().map_err(EncodeError::Attachment)?;
            encoder.encode_owned(File(fd))?;
        }
        Ok(())
    }
}

impl Decode for Core_AnonymousBuffer {
    const MIN_ENCODED_SIZE: usize = 1;

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        if !decoder.decode::<bool>()? {
            return Ok(Self::default());
        }
        let size = decoder.decode()?;
        let File(fd) = decoder.decode()?;
        Ok(Self {
            shared_memory: Some((size, Arc::new(fd))),
        })
    }
}

/// `URL::Host`: an `AK::Variant` of an IPv4 address, an IPv6 address (as a span of its 16 bytes), a domain and an
/// opaque host.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum URL_Host {
    Ipv4(u32),
    Ipv6(Vec<u8>),
    Domain(Utf8String),
    Opaque(Utf8String),
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct URL_OriginTuple {
    pub scheme: Option<Utf8String>,
    pub host: URL_Host,
    pub port: Option<u16>,
    pub domain: Option<URL_Host>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct URL_OpaqueOrigin {
    pub nonce: [u8; 16],
    pub kind: u8,
}

/// `URL::Origin`: whether it is opaque, as a bool, then what the origin is. A bool goes as the u8 index of a variant.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum URL_Origin {
    Tuple(URL_OriginTuple),
    Opaque(URL_OpaqueOrigin),
}

/// `URL::BlobURLEntry`, as it crosses processes: the token of its blob, if it is one, and the origin it was made in.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct URL_BlobURLEntry {
    pub blob_token: Option<u64>,
    pub origin: URL_Origin,
}

/// `URL::URL`: its serialization, which the receiver parses again, then its blob URL entry, if any.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct URL_URL {
    pub serialization: Utf8String,
    pub blob_url_entry: Option<URL_BlobURLEntry>,
}

impl URL_URL {
    /// A URL that the UI makes. The serialization must be one that the WHATWG basic URL parser accepts.
    pub fn from_serialization(serialization: impl Into<String>) -> Self {
        Self {
            serialization: Utf8String::from(serialization.into()),
            blob_url_entry: None,
        }
    }

    pub fn as_str(&self) -> &str {
        self.serialization.as_str().unwrap_or_default()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Web_HTML_CrossProcessId {
    pub namespace_id: u64,
    pub local_id: u64,
}

#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct Web_HTML_PictureInPictureWindowHints {
    pub navigable_id: Web_HTML_CrossProcessId,
    pub video_size: Gfx_IntSize,
}

#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct Web_HTML_WebViewHints {
    pub popup: bool,
    pub picture_in_picture_window: Option<Web_HTML_PictureInPictureWindowHints>,
    pub width: Option<Web_DevicePixels>,
    pub height: Option<Web_DevicePixels>,
    pub screen_x: Option<Web_DevicePixels>,
    pub screen_y: Option<Web_DevicePixels>,
}

/// The values of `Gfx::BitmapFormat`.
pub mod bitmap_format {
    pub const BGRX8888: u32 = 1;
    pub const BGRA8888: u32 = 2;
    pub const RGBX8888: u32 = 3;
    pub const RGBA8888: u32 = 4;
}

/// The values of `Gfx::AlphaType`.
pub mod alpha_type {
    pub const PREMULTIPLIED: u32 = 0;
    pub const UNPREMULTIPLIED: u32 = 1;
}

/// A bitmap in shared memory. Its rows are `width * 4` bytes apart, as `Gfx::Bitmap::minimum_pitch()` has them.
#[derive(Debug)]
pub struct SharedBitmap {
    pub fd: OwnedFd,
    pub size: Gfx_IntSize,
    pub format: u32,
    pub alpha_type: u32,
}

impl SharedBitmap {
    pub fn pitch(&self) -> usize {
        self.size.width.max(0) as usize * 4
    }

    pub fn size_in_bytes(&self) -> usize {
        self.pitch() * self.size.height.max(0) as usize
    }
}

/// `Gfx::ShareableBitmap`: whether it holds a bitmap, then a descriptor of its memory, its size, format and alpha type.
#[derive(Debug, Default)]
pub struct Gfx_ShareableBitmap(pub Option<SharedBitmap>);

impl Encode for Gfx_ShareableBitmap {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), EncodeError> {
        encoder.encode(&self.0.is_some())?;
        if let Some(bitmap) = &self.0 {
            let fd = bitmap.fd.try_clone().map_err(EncodeError::Attachment)?;
            encoder.encode_owned(File(fd))?;
            encoder.encode(&bitmap.size)?;
            encoder.encode(&bitmap.format)?;
            encoder.encode(&bitmap.alpha_type)?;
        }
        Ok(())
    }
}

impl Decode for Gfx_ShareableBitmap {
    const MIN_ENCODED_SIZE: usize = 1;

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        if !decoder.decode::<bool>()? {
            return Ok(Self(None));
        }
        let File(fd) = decoder.decode()?;
        let size: Gfx_IntSize = decoder.decode()?;
        if size.width <= 0 || size.height <= 0 {
            return Err(DecodeError::InvalidValue("Gfx::ShareableBitmap size"));
        }
        let format = decoder.decode()?;
        if !(bitmap_format::BGRX8888..=bitmap_format::RGBA8888).contains(&format) {
            return Err(DecodeError::InvalidValue("Gfx::ShareableBitmap format"));
        }
        let alpha_type = decoder.decode()?;
        if alpha_type > alpha_type::UNPREMULTIPLIED {
            return Err(DecodeError::InvalidValue("Gfx::ShareableBitmap alpha type"));
        }
        Ok(Self(Some(SharedBitmap {
            fd,
            size,
            format,
            alpha_type,
        })))
    }
}

#[derive(Debug, Encode, Decode)]
pub struct Gfx_LinuxDmaBufPlane {
    pub object_index: u32,
    pub offset: u32,
    pub pitch: u32,
}

/// `Gfx::LinuxDmaBufHandle`: its size, DRM format and modifier, its planes and its objects, each list after its count
/// as a size_t.
#[derive(Debug)]
pub struct Gfx_LinuxDmaBufHandle {
    pub size: Gfx_IntSize,
    pub drm_format: u32,
    pub modifier: u64,
    pub planes: Vec<Gfx_LinuxDmaBufPlane>,
    pub objects: Vec<File>,
}

const MAXIMUM_DMA_BUF_PLANE_COUNT: usize = 4;

impl Encode for Gfx_LinuxDmaBufHandle {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), EncodeError> {
        encoder.encode(&self.size)?;
        encoder.encode(&self.drm_format)?;
        encoder.encode(&self.modifier)?;
        encoder.encode(&self.planes.len())?;
        for plane in &self.planes {
            encoder.encode(plane)?;
        }
        encoder.encode(&self.objects.len())?;
        for object in &self.objects {
            encoder.encode(object)?;
        }
        Ok(())
    }
}

impl Decode for Gfx_LinuxDmaBufHandle {
    fn decode(decoder: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        let size = decoder.decode()?;
        let drm_format = decoder.decode()?;
        let modifier = decoder.decode()?;
        let plane_count: usize = decoder.decode()?;
        if plane_count == 0 || plane_count > MAXIMUM_DMA_BUF_PLANE_COUNT {
            return Err(DecodeError::InvalidValue("dma-buf plane count"));
        }
        let planes = (0..plane_count)
            .map(|_| decoder.decode())
            .collect::<Result<Vec<_>, _>>()?;
        let object_count: usize = decoder.decode()?;
        if object_count == 0 || object_count > MAXIMUM_DMA_BUF_PLANE_COUNT {
            return Err(DecodeError::InvalidValue("dma-buf object count"));
        }
        let objects = (0..object_count)
            .map(|_| decoder.decode())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            size,
            drm_format,
            modifier,
            planes,
            objects,
        })
    }
}

#[derive(Debug, Encode, Decode)]
pub struct Gfx_WindowsD3DHandle {
    pub size: Gfx_IntSize,
    pub file: File,
}

/// `Gfx::SharedImage`, off macOS: the kind of memory it is in, as a u8, then the image.
#[derive(Debug, Encode, Decode)]
pub enum Gfx_SharedImage {
    ShareableBitmap(Gfx_ShareableBitmap),
    LinuxDmaBuf(Gfx_LinuxDmaBufHandle),
    WindowsD3DTexture(Gfx_WindowsD3DHandle),
}

wire_enum!(Gfx_StandardCursor: i32 {
    None,
    Hidden,
    Arrow,
    Crosshair,
    IBeam,
    ResizeHorizontal,
    ResizeVertical,
    ResizeDiagonalTLBR,
    ResizeDiagonalBLTR,
    ResizeColumn,
    ResizeRow,
    Hand,
    Help,
    OpenHand,
    Drag,
    DragCopy,
    Move,
    Wait,
    Disallowed,
    Eyedropper,
    ZoomIn,
    ZoomOut,
});

#[derive(Debug, Encode, Decode)]
pub struct Gfx_ImageCursor {
    pub bitmap: Gfx_ShareableBitmap,
    pub hotspot: Gfx_IntPoint,
}

/// `Gfx::Cursor`: an `AK::Variant` of a standard cursor and an image.
#[derive(Debug, Encode, Decode)]
pub enum Gfx_Cursor {
    Standard(Gfx_StandardCursor),
    Image(Gfx_ImageCursor),
}

include!(concat!(env!("OUT_DIR"), "/HubProtocol.rs"));
