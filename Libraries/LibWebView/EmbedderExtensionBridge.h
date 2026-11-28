/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

#pragma once

#include <AK/ByteBuffer.h>
#include <AK/ByteString.h>
#include <AK/Function.h>
#include <AK/HashMap.h>
#include <AK/HashTable.h>
#include <AK/JsonObject.h>
#include <AK/NonnullOwnPtr.h>
#include <AK/Optional.h>
#include <AK/RefPtr.h>
#include <AK/String.h>
#include <AK/Vector.h>
#include <AK/WeakPtr.h>
#include <AK/Weakable.h>
#include <LibCore/Notifier.h>
#include <LibCore/Timer.h>
#include <LibRequests/Forward.h>
#include <LibURL/URL.h>
#include <LibWebView/Forward.h>

namespace WebView {

class HeadlessWebView;

// Thin Hub-side bridge over the generic embedder channel to Zendia's sandboxed ExtensionServer process.
//
// Dormant until the UI connects an embedder channel named "extensions" via HubServer::connect_embedder_channel.
// Holds no cgx-webext code or FFI: all extension policy, storage, manifest parsing, and API dispatch live in
// ExtensionServer. If ExtensionServer exits or crashes, this bridge cleanly disconnects and answers WebContent
// requests with empty results until the UI reconnects a restarted ExtensionServer.
class WEBVIEW_API EmbedderExtensionBridge final : public Weakable<EmbedderExtensionBridge> {
    AK_ALLOC_WITH_KMALLOC;
    AK_MAKE_NONCOPYABLE(EmbedderExtensionBridge);
    AK_MAKE_NONMOVABLE(EmbedderExtensionBridge);

public:
    static NonnullOwnPtr<EmbedderExtensionBridge> create(int socket_fd);
    ~EmbedderExtensionBridge();

    bool is_connected() const { return m_fd >= 0; }
    void disconnect();

    // Packed resource response for WebContentClient::did_request_extension_resource:
    // u32 LE meta_length | meta JSON {status, mime, csp} | body; empty if unhandled.
    ByteBuffer serve_resource(URL::URL const& url, StringView initiator_origin);

    // JSON array of content-script programs matching a document.
    String content_script_programs(StringView url, bool is_top_frame);

    struct ContextInitResult {
        bool ok { false };
        bool misbehaved { false };
        u64 context_id { 0 };
        ByteString extension_id;
        String init_json;
    };
    ContextInitResult context_init(
        StringView url,
        StringView origin,
        StringView bridge_key,
        bool is_top_frame,
        i64 frame_id,
        Optional<i64> tab_id,
        bool is_popup,
        bool has_injection_grant);

    void context_destroyed(u64 context_id);

    Optional<String> api_call_sync(u64 context_id, StringView name, StringView args_json);
    void api_call(u64 context_id, u64 request_id, u64 view_id, StringView name, StringView args_json);

    void report_network_stage(WeakPtr<WebContentClient> client, u64 token, StringView stage, bool is_tab, JsonObject const& details);

    u32 network_interest() const { return m_network_interest; }
    bool is_background_view(u64 view_id) const;
    bool is_popup_view(u64 view_id) const;

private:
    explicit EmbedderExtensionBridge(int socket_fd);

    enum class FrameKind : u8 {
        Request = 0,
        Response = 1,
        Event = 2,
    };

    struct Frame {
        FrameKind kind { FrameKind::Event };
        u64 id { 0 };
        JsonObject json;
        ByteBuffer binary;
    };

    struct PendingDownload {
        AK_ALLOC_WITH_KMALLOC;
        RefPtr<Requests::Request> request;
        RefPtr<Core::Timer> timeout;
    };

    bool write_frame(FrameKind kind, u64 id, JsonObject const& json, ReadonlyBytes binary = {});
    bool send_event(JsonObject const& json, ReadonlyBytes binary = {});
    Optional<Frame> send_sync(JsonObject const& json, ReadonlyBytes binary = {});

    void on_socket_readable();
    bool drain_socket_nonblocking();
    Optional<Frame> try_parse_frame();
    void dispatch_incoming_frame(Frame const&);
    void flush_deferred_frames();

    void handle_start_background_page(ByteString const& extension_id, StringView url_string);
    void handle_stop_background_page(ByteString const& extension_id);
    void handle_reload_extension_pages(StringView base_url);
    void handle_hub_host_call(u64 token, ByteString const& extension_id, StringView function, JsonObject const& args);
    void handle_download_url(u64 token, URL::URL url, u32 redirects = 0);

    int m_fd { -1 };
    RefPtr<Core::Notifier> m_notifier;
    ByteBuffer m_read_buffer;
    Vector<Frame> m_deferred_frames;
    bool m_flush_scheduled { false };
    u64 m_next_request_id { 1 };

    u32 m_network_interest { 0 };
    HashMap<ByteString, NonnullOwnPtr<HeadlessWebView>> m_background_views;
    HashTable<u64> m_popup_views;
    // Each WebContent process numbers its own tokens, so the same token can come from two processes at once. The
    // ExtensionServer sees the bridge's own number instead, which maps back to the process and its token.
    struct PendingNetworkAnswer {
        WeakPtr<WebContentClient> client;
        u64 token { 0 };
    };
    HashMap<u64, PendingNetworkAnswer> m_pending_network_answers;
    u64 m_next_network_token { 1 };
    HashMap<u64, NonnullOwnPtr<PendingDownload>> m_downloads;
};

}
