/*
 * Copyright (c) 2026-present, the Ladybird developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The types the Hub's messages carry, under the names the IPC compiler gives them. Most are generated from their .ipc
//! declarations by generate.py. The ones below have no declaration, so their wire forms are written out here, each as
//! its C++ `IPC::encode()` and `IPC::decode()` have it.

#![allow(non_camel_case_types)]

use libipc_rust::wire::{
    ByteString, Decode, DecodeError, Decoder, Encode, EncodeError, Encoder, File, Utf8String, Utf16String,
};
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
wire_enum!(AK_CaseSensitivity: i32 { CaseInsensitive, CaseSensitive });
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

impl Clone for Gfx_ShareableBitmap {
    fn clone(&self) -> Self {
        let cloned = self.0.as_ref().and_then(|bitmap| {
            let fd = bitmap.fd.try_clone().ok()?;
            Some(SharedBitmap {
                fd,
                size: bitmap.size.clone(),
                format: bitmap.format,
                alpha_type: bitmap.alpha_type,
            })
        });
        Self(cloned)
    }
}

impl PartialEq for Gfx_ShareableBitmap {
    fn eq(&self, other: &Self) -> bool {
        use std::os::fd::AsRawFd;
        match (&self.0, &other.0) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                a.fd.as_raw_fd() == b.fd.as_raw_fd()
                    && a.size == b.size
                    && a.format == b.format
                    && a.alpha_type == b.alpha_type
            }
            _ => false,
        }
    }
}

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

// Context menu, media action, clipboard paste, and `<select>` dropdown wire types.
wire_enum!(WebView_ContextMenuKind: u8 { Page, Link, Image, Media });
wire_enum!(WebView_ContextMenuMediaAction: u8 {
    TogglePlay,
    ToggleMute,
    ToggleControls,
    ToggleLoop,
    TogglePictureInPicture,
    ToggleFullscreen,
});
wire_enum!(Web_Clipboard_PasteMode: u8 { AllMimeTypes, PlainTextOnly });

/// `Web::MediaContextMenu` (field order matches `Libraries/LibWebCommon/Page/MediaContextMenu.cpp`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
pub struct Web_MediaContextMenu {
    pub media_url: Option<URL_URL>,
    pub is_video: bool,
    pub is_playing: bool,
    pub is_muted: bool,
    pub has_user_agent_controls: bool,
    pub is_looping: bool,
    pub is_fullscreen: bool,
    pub is_picture_in_picture: bool,
    pub can_enter_picture_in_picture: bool,
}

/// `Web::HTML::SelectItemOption` (wire field order matches `Libraries/LibWebCommon/HTML/SelectItem.cpp`:
/// `id`, `label`, `value`, `selected`, `disabled`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
pub struct Web_HTML_SelectItemOption {
    pub id: u32,
    pub label: Utf16String,
    pub value: Utf16String,
    pub selected: bool,
    pub disabled: bool,
}

/// `Web::HTML::SelectItemOptionGroup`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
pub struct Web_HTML_SelectItemOptionGroup {
    pub label: Utf16String,
    pub items: Vec<Web_HTML_SelectItemOption>,
}

/// `Web::HTML::SelectItemSeparator` (empty struct on the wire).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Web_HTML_SelectItemSeparator;

impl Encode for Web_HTML_SelectItemSeparator {
    fn encode(&self, _encoder: &mut Encoder) -> Result<(), EncodeError> {
        Ok(())
    }
}

impl Decode for Web_HTML_SelectItemSeparator {
    const MIN_ENCODED_SIZE: usize = 0;

    fn decode(_decoder: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self)
    }
}

/// `Web::HTML::SelectItem`: `AK::Variant<SelectItemOption, SelectItemOptionGroup, SelectItemSeparator>`.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum Web_HTML_SelectItem {
    Option(Web_HTML_SelectItemOption),
    OptionGroup(Web_HTML_SelectItemOptionGroup),
    Separator(Web_HTML_SelectItemSeparator),
}

// `Web::HTML::PreparedNavigationDescriptor` and its nested wire types.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AK_Empty;

impl Encode for AK_Empty {
    fn encode(&self, _encoder: &mut Encoder) -> Result<(), EncodeError> {
        Ok(())
    }
}

impl Decode for AK_Empty {
    const MIN_ENCODED_SIZE: usize = 0;

    fn decode(_decoder: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self)
    }
}

wire_enum!(Web_HTML_POSTResource_RequestContentType: i32 {
    ApplicationXWWWFormUrlencoded,
    MultipartFormData,
    TextPlain,
});
wire_enum!(Web_ReferrerPolicy_ReferrerPolicy: i32 {
    EmptyString,
    NoReferrer,
    NoReferrerWhenDowngrade,
    SameOrigin,
    Origin,
    StrictOrigin,
    OriginWhenCrossOrigin,
    StrictOriginWhenCrossOrigin,
    UnsafeURL,
});
wire_enum!(Web_HTML_UserNavigationInvolvement: i32 { BrowserUI, Activation, None });
wire_enum!(Web_HTML_InitialInsertion: u8 { Yes, No });
wire_enum!(Web_ContentSecurityPolicy_Directives_NavigationType: i32 { FormSubmission, Other });
wire_enum!(Web_Bindings_SecurityPolicyViolationEventDisposition: u8 { Enforce, Report });
wire_enum!(Web_ContentSecurityPolicy_PolicySource: i32 { Header, Meta });
wire_enum!(Web_HTML_EmbedderPolicyValue: u8 { UnsafeNone, RequireCorp, Credentialless });
wire_enum!(Web_UpgradeInsecureRequests_InsecureRequestsPolicy: u8 { DoNotUpgrade, Upgrade });
wire_enum!(Web_HTML_CanUseCrossOriginIsolatedAPIs: u8 { No, Yes });
wire_enum!(Web_MixedContent_ProhibitsMixedSecurityContexts: u8 {
    ProhibitsMixedSecurityContexts,
    DoesNotRestrictMixedSecurityContexts,
});
wire_enum!(Web_HTML_CrossOriginIsolationMode: u8 { None, Logical, Concrete });

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Web_HTML_POSTResource_Directive {
    pub r#type: Utf16String,
    pub value: Utf16String,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Web_HTML_POSTResource {
    pub request_body: Option<ByteString>,
    pub request_content_type: Web_HTML_POSTResource_RequestContentType,
    pub request_content_type_directives: Vec<Web_HTML_POSTResource_Directive>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum Web_HTML_DocumentResource {
    Empty(AK_Empty),
    String(Utf16String),
    Post(Web_HTML_POSTResource),
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Web_HTML_StorageSerializationRecord {
    pub data: ByteString,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Web_ContentSecurityPolicy_Directives_SerializedDirective {
    pub name: Utf16String,
    pub value: Vec<Utf16String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Web_ContentSecurityPolicy_SerializedPolicy {
    pub directives: Vec<Web_ContentSecurityPolicy_Directives_SerializedDirective>,
    pub disposition: Web_Bindings_SecurityPolicyViolationEventDisposition,
    pub source: Web_ContentSecurityPolicy_PolicySource,
    pub self_origin: URL_Origin,
    pub pre_parsed_policy_string: Utf8String,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Web_HTML_EmbedderPolicy {
    pub value: Web_HTML_EmbedderPolicyValue,
    pub reporting_endpoint: Utf16String,
    pub report_only_value: Web_HTML_EmbedderPolicyValue,
    pub report_only_reporting_endpoint: Utf16String,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Web_UpgradeInsecureRequests_HostAndPort {
    pub host: URL_Host,
    pub port: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Web_HTML_SerializedPolicyContainer {
    pub csp_list: Vec<Web_ContentSecurityPolicy_SerializedPolicy>,
    pub embedder_policy: Web_HTML_EmbedderPolicy,
    pub referrer_policy: Web_ReferrerPolicy_ReferrerPolicy,
    pub force_load_at_top: bool,
    pub insecure_requests_policy: Web_UpgradeInsecureRequests_InsecureRequestsPolicy,
    pub upgrade_insecure_navigations_set: Vec<Web_UpgradeInsecureRequests_HostAndPort>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Web_HTML_SerializedWindow {
    pub document_url: URL_URL,
    pub site_for_cookies: URL_Origin,
    pub relevant_settings_object_is_secure_context: bool,
    pub relevant_settings_object_policy_container: Web_HTML_SerializedPolicyContainer,
    pub navigable_is_top_level: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Web_HTML_SerializedWorkerGlobalScope {
    pub site_for_cookies: URL_Origin,
    pub relevant_settings_object_is_secure_context: bool,
    pub relevant_settings_object_policy_container: Web_HTML_SerializedPolicyContainer,
    pub is_supported_animation_frame_provider: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum Web_HTML_SerializedGlobal {
    Window(Web_HTML_SerializedWindow),
    Worker(Web_HTML_SerializedWorkerGlobalScope),
}

#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct Web_HTML_SerializedEnvironmentSettingsObject {
    pub id: Utf8String,
    pub creation_url: URL_URL,
    pub top_level_creation_url: Option<URL_URL>,
    pub top_level_origin: Option<URL_Origin>,
    pub api_base_url: URL_URL,
    pub origin: URL_Origin,
    pub has_cross_site_ancestor: bool,
    pub policy_container: Web_HTML_SerializedPolicyContainer,
    pub cross_origin_isolated_capability: Web_HTML_CanUseCrossOriginIsolatedAPIs,
    pub prohibits_mixed_security_contexts: Web_MixedContent_ProhibitsMixedSecurityContexts,
    pub agent_cluster_id: Option<u64>,
    pub agent_cluster_cross_origin_isolation_mode: Web_HTML_CrossOriginIsolationMode,
    pub time_origin: f64,
    pub global: Web_HTML_SerializedGlobal,
}

#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct Web_HTML_NavigationSourceSnapshot {
    pub has_transient_activation: bool,
    pub sandboxing_flags: u32,
    pub allows_downloading: bool,
    pub fetch_client: Option<Web_HTML_SerializedEnvironmentSettingsObject>,
    pub source_policy_container: Web_HTML_SerializedPolicyContainer,
    pub text_directive_user_activation: bool,
    pub browsing_context_group_has_multiple_contexts: bool,
    pub browsing_context_group_has_remote_contexts: bool,
}

#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct Web_HTML_PreparedNavigationDescriptor {
    pub url: URL_URL,
    pub document_resource: Web_HTML_DocumentResource,
    pub history_handling: Web_Bindings_NavigationHistoryBehavior,
    pub navigation_api_state: Option<Web_HTML_StorageSerializationRecord>,
    pub referrer_policy: Web_ReferrerPolicy_ReferrerPolicy,
    pub user_involvement: Web_HTML_UserNavigationInvolvement,
    pub navigation_id: Utf16String,
    pub initial_insertion: Web_HTML_InitialInsertion,
    pub csp_navigation_type: Web_ContentSecurityPolicy_Directives_NavigationType,
    pub source_snapshot_params: Web_HTML_NavigationSourceSnapshot,
    pub initiator_origin_snapshot: URL_Origin,
    pub initiator_base_url_snapshot: URL_URL,
}

include!(concat!(env!("OUT_DIR"), "/HubProtocol.rs"));
