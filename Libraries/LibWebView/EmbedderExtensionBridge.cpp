/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

#include <AK/Endian.h>
#include <AK/JsonArray.h>
#include <AK/JsonValue.h>
#include <AK/LexicalPath.h>
#include <AK/Time.h>
#include <LibCore/EventLoop.h>
#include <LibGfx/SystemTheme.h>
#include <LibHTTP/Cookie/Cookie.h>
#include <LibHTTP/HeaderList.h>
#include <LibRequests/NetworkError.h>
#include <LibRequests/Request.h>
#include <LibRequests/RequestClient.h>
#include <LibURL/Parser.h>
#include <LibWebCommon/Fetch/Infrastructure/HTTP/Statuses.h>
#include <LibWebCommon/Loader/UserAgent.h>
#include <LibWebView/Application.h>
#include <LibWebView/BrowsingSession.h>
#include <LibWebView/CookieJar.h>
#include <LibWebView/EmbedderExtensionBridge.h>
#include <LibWebView/HeadlessWebView.h>
#include <LibWebView/SearchEngine.h>
#include <LibWebView/Settings.h>
#include <LibWebView/Utilities.h>
#include <LibWebView/ViewImplementation.h>
#include <LibWebView/WebContentClient.h>
#include <LibWebView/WebContentPage.h>

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <sys/socket.h>
#include <unistd.h>

namespace WebView {

static constexpr size_t MAX_BRIDGE_FRAME_BYTES = 64 * MiB;
static constexpr size_t MAX_STORE_PACKAGE_BYTES = 256 * MiB;
static constexpr u32 MAX_DOWNLOAD_REDIRECTS = 10;
static constexpr int SYNC_TIMEOUT_MS = 5000;
static constexpr auto DOWNLOAD_TIMEOUT = AK::Duration::from_seconds(120);

NonnullOwnPtr<EmbedderExtensionBridge> EmbedderExtensionBridge::create(int socket_fd)
{
    return adopt_own(*new EmbedderExtensionBridge(socket_fd));
}

EmbedderExtensionBridge::EmbedderExtensionBridge(int socket_fd)
    : m_fd(socket_fd)
{
    if (m_fd >= 0) {
        int flags = fcntl(m_fd, F_GETFL, 0);
        if (flags >= 0)
            fcntl(m_fd, F_SETFL, flags | O_NONBLOCK);
        m_notifier = Core::Notifier::construct(m_fd, Core::Notifier::Type::Read);
        m_notifier->on_activation = [this] { on_socket_readable(); };
    }
}

EmbedderExtensionBridge::~EmbedderExtensionBridge()
{
    disconnect();
}

void EmbedderExtensionBridge::disconnect()
{
    if (m_notifier) {
        m_notifier->set_enabled(false);
        m_notifier = nullptr;
    }
    if (m_fd >= 0) {
        close(m_fd);
        m_fd = -1;
    }
    m_read_buffer.clear();
    m_deferred_frames.clear();
    m_background_views.clear();
    m_popup_views.clear();

    // Release any WebContent fetches waiting on a network answer from the dead ExtensionServer.
    auto pending_net = move(m_pending_network_answers);
    for (auto const& [token, weak_client] : pending_net) {
        if (auto client = weak_client.strong_ref())
            client->async_extension_network_answer(token, "{}"_string);
    }

    for (auto& [id, download] : m_downloads) {
        if (download->timeout)
            download->timeout->stop();
        if (download->request)
            download->request->stop();
    }
    m_downloads.clear();

    if (m_network_interest != 0) {
        m_network_interest = 0;
        WebContentClient::for_each_client([](WebContentClient& client) {
            client.async_set_extension_network_interest(0);
            return IterationDecision::Continue;
        });
    }
}

static bool write_all_with_poll(int fd, ReadonlyBytes bytes, int timeout_ms)
{
    size_t written = 0;
    while (written < bytes.size()) {
        ssize_t n = send(fd, bytes.data() + written, bytes.size() - written, MSG_NOSIGNAL);
        if (n > 0) {
            written += static_cast<size_t>(n);
            continue;
        }
        if (n < 0 && errno == EINTR)
            continue;
        if (n < 0 && errno == EAGAIN) {
            pollfd pfd { .fd = fd, .events = POLLOUT, .revents = 0 };
            int ready = poll(&pfd, 1, timeout_ms);
            if (ready <= 0)
                return false;
            continue;
        }
        return false;
    }
    return true;
}

bool EmbedderExtensionBridge::write_frame(FrameKind kind, u64 id, JsonObject const& json, ReadonlyBytes binary)
{
    if (m_fd < 0)
        return false;

    auto json_bytes = json.serialized().to_byte_string();
    u64 body_len_u64 = 1 + 8 + 4 + json_bytes.length() + binary.size();
    if (body_len_u64 > MAX_BRIDGE_FRAME_BYTES)
        return false;

    u8 header[17];
    u32 body_len = AK::convert_between_host_and_little_endian(static_cast<u32>(body_len_u64));
    u64 id_le = AK::convert_between_host_and_little_endian(id);
    u32 json_len = AK::convert_between_host_and_little_endian(static_cast<u32>(json_bytes.length()));

    memcpy(header + 0, &body_len, 4);
    header[4] = static_cast<u8>(kind);
    memcpy(header + 5, &id_le, 8);
    memcpy(header + 13, &json_len, 4);

    if (!write_all_with_poll(m_fd, { header, sizeof(header) }, SYNC_TIMEOUT_MS)
        || !write_all_with_poll(m_fd, json_bytes.bytes(), SYNC_TIMEOUT_MS)
        || (!binary.is_empty() && !write_all_with_poll(m_fd, binary, SYNC_TIMEOUT_MS))) {
        disconnect();
        return false;
    }
    return true;
}

bool EmbedderExtensionBridge::send_event(JsonObject const& json, ReadonlyBytes binary)
{
    return write_frame(FrameKind::Event, 0, json, binary);
}

bool EmbedderExtensionBridge::drain_socket_nonblocking()
{
    if (m_fd < 0)
        return false;

    u8 chunk[16384];
    for (;;) {
        ssize_t n = recv(m_fd, chunk, sizeof(chunk), MSG_DONTWAIT);
        if (n > 0) {
            if (m_read_buffer.size() + static_cast<size_t>(n) > MAX_BRIDGE_FRAME_BYTES + 4) {
                disconnect();
                return false;
            }
            if (m_read_buffer.try_append(chunk, static_cast<size_t>(n)).is_error()) {
                disconnect();
                return false;
            }
            continue;
        }
        if (n == 0) {
            disconnect();
            return false;
        }
        if (errno == EINTR)
            continue;
        if (errno == EAGAIN)
            return true;
        disconnect();
        return false;
    }
}

Optional<EmbedderExtensionBridge::Frame> EmbedderExtensionBridge::try_parse_frame()
{
    if (m_read_buffer.size() < 4)
        return {};

    u32 body_len_raw = 0;
    memcpy(&body_len_raw, m_read_buffer.data(), 4);
    u32 body_len = AK::convert_between_host_and_little_endian(body_len_raw);
    if (body_len < 13 || body_len > MAX_BRIDGE_FRAME_BYTES) {
        disconnect();
        return {};
    }
    if (m_read_buffer.size() < 4u + body_len)
        return {};

    auto const* body = m_read_buffer.data() + 4;
    u8 kind_byte = body[0];
    if (kind_byte > 2) {
        disconnect();
        return {};
    }
    u64 id_raw = 0;
    memcpy(&id_raw, body + 1, 8);
    u32 json_len_raw = 0;
    memcpy(&json_len_raw, body + 9, 4);
    u64 id = AK::convert_between_host_and_little_endian(id_raw);
    u32 json_len = AK::convert_between_host_and_little_endian(json_len_raw);
    if (13u + json_len > body_len) {
        disconnect();
        return {};
    }

    StringView json_view { reinterpret_cast<char const*>(body + 13), json_len };
    auto parsed = JsonValue::from_string(json_view);
    JsonObject obj;
    if (!parsed.is_error() && parsed.value().is_object())
        obj = parsed.release_value().as_object();

    size_t bin_offset = 13u + json_len;
    size_t bin_len = body_len - bin_offset;
    ByteBuffer binary;
    if (bin_len > 0) {
        auto copied = ByteBuffer::copy(body + bin_offset, bin_len);
        if (copied.is_error()) {
            disconnect();
            return {};
        }
        binary = copied.release_value();
    }

    size_t total_consumed = 4u + body_len;
    size_t remaining = m_read_buffer.size() - total_consumed;
    if (remaining > 0)
        memmove(m_read_buffer.data(), m_read_buffer.data() + total_consumed, remaining);
    m_read_buffer.resize(remaining);

    return Frame {
        .kind = static_cast<FrameKind>(kind_byte),
        .id = id,
        .json = move(obj),
        .binary = move(binary),
    };
}

Optional<EmbedderExtensionBridge::Frame> EmbedderExtensionBridge::send_sync(JsonObject const& json, ReadonlyBytes binary)
{
    if (m_fd < 0)
        return {};

    u64 req_id = m_next_request_id++;
    if (!write_frame(FrameKind::Request, req_id, json, binary))
        return {};

    auto deadline = MonotonicTime::now() + AK::Duration::from_milliseconds(SYNC_TIMEOUT_MS);
    for (;;) {
        for (auto frame = try_parse_frame(); frame.has_value(); frame = try_parse_frame()) {
            if (frame->kind == FrameKind::Response && frame->id == req_id) {
                if (!m_deferred_frames.is_empty() && !m_flush_scheduled) {
                    m_flush_scheduled = true;
                    Core::deferred_invoke([weak = make_weak_ptr()] {
                        if (weak)
                            weak->flush_deferred_frames();
                    });
                }
                return frame;
            }
            m_deferred_frames.append(frame.release_value());
        }
        if (m_fd < 0)
            return {};

        auto remaining = deadline - MonotonicTime::now();
        if (remaining <= AK::Duration::zero()) {
            warnln("EmbedderExtensionBridge: timed out waiting for ExtensionServer response");
            disconnect();
            return {};
        }
        pollfd pfd { .fd = m_fd, .events = POLLIN, .revents = 0 };
        int ready = poll(&pfd, 1, static_cast<int>(remaining.to_milliseconds()));
        if (ready < 0) {
            if (errno == EINTR)
                continue;
            disconnect();
            return {};
        }
        if (ready == 0) {
            warnln("EmbedderExtensionBridge: poll timed out waiting for ExtensionServer response");
            disconnect();
            return {};
        }
        if (!drain_socket_nonblocking())
            return {};
    }
}

void EmbedderExtensionBridge::on_socket_readable()
{
    if (!drain_socket_nonblocking())
        return;
    for (auto frame = try_parse_frame(); frame.has_value(); frame = try_parse_frame())
        dispatch_incoming_frame(*frame);
    flush_deferred_frames();
}

void EmbedderExtensionBridge::flush_deferred_frames()
{
    m_flush_scheduled = false;
    while (!m_deferred_frames.is_empty() && m_fd >= 0) {
        auto frames = move(m_deferred_frames);
        for (auto const& frame : frames) {
            if (m_fd < 0)
                return;
            dispatch_incoming_frame(frame);
        }
    }
}

ByteBuffer EmbedderExtensionBridge::serve_resource(URL::URL const& url, StringView initiator_origin)
{
    if (m_fd < 0)
        return {};
    JsonObject req;
    req.set("op"sv, "serve"sv);
    req.set("url"sv, url.serialize());
    req.set("initiator"sv, MUST(String::from_utf8(initiator_origin)));

    auto reply = send_sync(req);
    if (!reply.has_value() || !reply->json.get_bool("ok"sv).value_or(false))
        return {};
    auto data = reply->json.get_object("data"sv);
    if (!data.has_value() || !data->get_bool("found"sv).value_or(false))
        return {};

    JsonObject meta;
    meta.set("status"sv, data->get_integer<u16>("status"sv).value_or(200));
    meta.set("mime"sv, data->get_string("mime"sv).value_or("application/octet-stream"_string));
    if (auto csp = data->get_string("csp"sv); csp.has_value())
        meta.set("csp"sv, *csp);
    auto meta_json = meta.serialized().to_byte_string();

    ByteBuffer buffer;
    u32 length = static_cast<u32>(meta_json.length());
    if (buffer.try_append(&length, 4).is_error()
        || buffer.try_append(meta_json.bytes()).is_error()
        || buffer.try_append(reply->binary.bytes()).is_error())
        return {};
    return buffer;
}

String EmbedderExtensionBridge::content_script_programs(StringView url, bool is_top_frame)
{
    if (m_fd < 0)
        return "[]"_string;
    JsonObject req;
    req.set("op"sv, "content_script_programs"sv);
    req.set("url"sv, MUST(String::from_utf8(url)));
    req.set("isTopFrame"sv, is_top_frame);

    auto reply = send_sync(req);
    if (!reply.has_value() || !reply->json.get_bool("ok"sv).value_or(false))
        return "[]"_string;
    auto data = reply->json.get_object("data"sv);
    if (!data.has_value())
        return "[]"_string;
    return data->get_string("programs"sv).value_or("[]"_string);
}

EmbedderExtensionBridge::ContextInitResult EmbedderExtensionBridge::context_init(
    StringView url,
    StringView origin,
    StringView bridge_key,
    bool is_top_frame,
    i64 frame_id,
    Optional<i64> tab_id,
    bool is_popup,
    bool has_injection_grant)
{
    if (m_fd < 0)
        return {};
    JsonObject req;
    req.set("op"sv, "context_init"sv);
    req.set("url"sv, MUST(String::from_utf8(url)));
    req.set("origin"sv, MUST(String::from_utf8(origin)));
    req.set("bridgeKey"sv, MUST(String::from_utf8(bridge_key)));
    req.set("isTopFrame"sv, is_top_frame);
    req.set("frameId"sv, frame_id);
    if (tab_id.has_value())
        req.set("tabId"sv, *tab_id);
    req.set("isPopup"sv, is_popup);
    req.set("hasInjectionGrant"sv, has_injection_grant);

    auto reply = send_sync(req);
    if (!reply.has_value() || !reply->json.get_bool("ok"sv).value_or(false))
        return {};
    auto data = reply->json.get_object("data"sv);
    if (!data.has_value())
        return {};

    ContextInitResult out;
    out.ok = data->get_bool("ok"sv).value_or(false);
    out.misbehaved = data->get_bool("misbehaved"sv).value_or(false);
    out.context_id = data->get_integer<u64>("contextId"sv).value_or(0);
    out.extension_id = data->get_string("extensionId"sv).value_or({}).to_byte_string();
    out.init_json = data->get_string("initJson"sv).value_or({});
    return out;
}

void EmbedderExtensionBridge::context_destroyed(u64 context_id)
{
    JsonObject ev;
    ev.set("op"sv, "context_destroyed"sv);
    ev.set("contextId"sv, context_id);
    send_event(ev);
}

Optional<String> EmbedderExtensionBridge::api_call_sync(u64 context_id, StringView name, StringView args_json)
{
    if (m_fd < 0)
        return {};
    JsonObject req;
    req.set("op"sv, "api_call_sync"sv);
    req.set("contextId"sv, context_id);
    req.set("name"sv, MUST(String::from_utf8(name)));
    req.set("argsJson"sv, MUST(String::from_utf8(args_json)));

    auto reply = send_sync(req);
    if (!reply.has_value() || !reply->json.get_bool("ok"sv).value_or(false))
        return {};
    auto data = reply->json.get_object("data"sv);
    if (!data.has_value())
        return {};
    return data->get_string("result"sv).copy();
}

void EmbedderExtensionBridge::api_call(u64 context_id, u64 request_id, u64 view_id, StringView name, StringView args_json)
{
    JsonObject ev;
    ev.set("op"sv, "api_call"sv);
    ev.set("contextId"sv, context_id);
    ev.set("requestId"sv, request_id);
    ev.set("viewId"sv, view_id);
    ev.set("name"sv, MUST(String::from_utf8(name)));
    ev.set("argsJson"sv, MUST(String::from_utf8(args_json)));
    send_event(ev);
}

void EmbedderExtensionBridge::report_network_stage(WeakPtr<WebContentClient> client, u64 token, StringView stage, bool is_tab, JsonObject const& details)
{
    if (m_fd < 0) {
        if (token != 0) {
            if (auto strong = client.strong_ref())
                strong->async_extension_network_answer(token, "{}"_string);
        }
        return;
    }
    if (token != 0)
        m_pending_network_answers.set(token, move(client));
    JsonObject ev;
    ev.set("op"sv, "network_stage"sv);
    ev.set("token"sv, token);
    ev.set("stage"sv, MUST(String::from_utf8(stage)));
    ev.set("isTab"sv, is_tab);
    ev.set("details"sv, details);
    send_event(ev);
}

bool EmbedderExtensionBridge::is_background_view(u64 view_id) const
{
    for (auto const& [id, view] : m_background_views) {
        if (view->view_id() == view_id)
            return true;
    }
    return false;
}

bool EmbedderExtensionBridge::is_popup_view(u64 view_id) const
{
    return m_popup_views.contains(view_id);
}

void EmbedderExtensionBridge::dispatch_incoming_frame(Frame const& frame)
{
    auto op = frame.json.get_string("op"sv).value_or({});
    if (op == "deliver_to_context"sv) {
        auto context_id = frame.json.get_integer<u64>("contextId"sv).value_or(0);
        auto json = frame.json.get_string("json"sv).value_or({});
        deliver_to_extension_context(context_id, json.bytes_as_string_view());
        return;
    }
    if (op == "deliver_to_extension"sv) {
        auto ext_id = frame.json.get_string("extensionId"sv).value_or({});
        auto json = frame.json.get_string("json"sv).value_or({});
        deliver_event_to_extension(ext_id.bytes_as_string_view(), json.bytes_as_string_view());
        return;
    }
    if (op == "api_reply"sv) {
        auto context_id = frame.json.get_integer<u64>("contextId"sv).value_or(0);
        auto request_id = frame.json.get_integer<u64>("requestId"sv).value_or(0);
        auto ok = frame.json.get_bool("ok"sv).value_or(false);
        auto json = frame.json.get_string("json"sv).value_or({});
        deliver_api_reply_to_extension_context(context_id, request_id, ok, json);
        return;
    }
    if (op == "set_network_interest"sv) {
        u32 stages = frame.json.get_integer<u32>("stages"sv).value_or(0);
        if (stages == m_network_interest)
            return;
        m_network_interest = stages;
        WebContentClient::for_each_client([stages](WebContentClient& client) {
            client.async_set_extension_network_interest(stages);
            return IterationDecision::Continue;
        });
        return;
    }
    if (op == "network_answer"sv) {
        auto token = frame.json.get_integer<u64>("token"sv).value_or(0);
        auto answer_json = frame.json.get_string("answerJson"sv).value_or("{}"_string);
        if (auto weak = m_pending_network_answers.take(token); weak.has_value()) {
            if (auto client = weak->strong_ref())
                client->async_extension_network_answer(token, answer_json);
        }
        return;
    }
    if (op == "start_background_page"sv) {
        auto ext_id = frame.json.get_string("extensionId"sv).value_or({}).to_byte_string();
        auto url = frame.json.get_string("url"sv).value_or({});
        handle_start_background_page(ext_id, url.bytes_as_string_view());
        return;
    }
    if (op == "stop_background_page"sv) {
        auto ext_id = frame.json.get_string("extensionId"sv).value_or({}).to_byte_string();
        handle_stop_background_page(ext_id);
        return;
    }
    if (op == "reload_extension_pages"sv) {
        auto base_url = frame.json.get_string("baseUrl"sv).value_or({});
        handle_reload_extension_pages(base_url.bytes_as_string_view());
        return;
    }
    if (op == "set_popup_view"sv) {
        auto view_id = frame.json.get_integer<u64>("viewId"sv).value_or(0);
        auto is_popup = frame.json.get_bool("isPopup"sv).value_or(false);
        if (is_popup)
            m_popup_views.set(view_id);
        else
            m_popup_views.remove(view_id);
        return;
    }
    if (op == "hub_host_call"sv) {
        auto token = frame.json.get_integer<u64>("token"sv).value_or(0);
        auto ext_id = frame.json.get_string("extensionId"sv).value_or({}).to_byte_string();
        auto function = frame.json.get_string("function"sv).value_or({});
        auto args = frame.json.get_object("args"sv).value_or(JsonObject {});
        handle_hub_host_call(token, ext_id, function.bytes_as_string_view(), args);
        return;
    }
    if (op == "download_url"sv) {
        auto token = frame.json.get_integer<u64>("token"sv).value_or(0);
        auto url_str = frame.json.get_string("url"sv).value_or({});
        auto url = URL::Parser::basic_parse(url_str);
        if (!url.has_value()) {
            JsonObject reply;
            reply.set("op"sv, "download_reply"sv);
            reply.set("token"sv, token);
            reply.set("ok"sv, false);
            reply.set("error"sv, "Invalid download URL"sv);
            send_event(reply);
            return;
        }
        handle_download_url(token, url.release_value());
        return;
    }
}

void EmbedderExtensionBridge::handle_start_background_page(ByteString const& extension_id, StringView url_string)
{
    if (m_background_views.contains(extension_id))
        return;
    auto url = URL::Parser::basic_parse(url_string);
    if (!url.has_value())
        return;

    auto theme_path = LexicalPath::join(WebView::s_ladybird_resource_root, "themes"sv, "Default.ini"sv);
    auto theme = Gfx::load_system_theme(theme_path.string());
    if (theme.is_error()) {
        warnln("Extensions: cannot load theme for background page: {}", theme.error());
        return;
    }

    auto view = HeadlessWebView::create(theme.value(), { 800, 600 });
    view->on_load_finish = [weak = make_weak_ptr(), extension_id, fired = false](URL::URL const&) mutable {
        if (exchange(fired, true) || !weak)
            return;
        JsonObject ev;
        ev.set("op"sv, "background_loaded"sv);
        ev.set("extensionId"sv, MUST(String::from_byte_string(extension_id)));
        weak->send_event(ev);
    };
    view->load_from_browser_ui(*url);
    m_background_views.set(extension_id, move(view));
}

void EmbedderExtensionBridge::handle_stop_background_page(ByteString const& extension_id)
{
    m_background_views.remove(extension_id);
}

void EmbedderExtensionBridge::handle_reload_extension_pages(StringView base_url)
{
    if (base_url.is_empty())
        return;
    ViewImplementation::for_each_view([&](ViewImplementation& view) {
        auto serialized = view.url().serialize();
        if (serialized.bytes_as_string_view().starts_with(base_url) && !is_popup_view(view.view_id()) && !is_background_view(view.view_id()))
            view.reload();
        return IterationDecision::Continue;
    });
}

static JsonObject serialize_extension_cookie(HTTP::Cookie::Cookie const& cookie, StringView store_id = "0"sv)
{
    JsonObject obj;
    obj.set("name"sv, cookie.name);
    obj.set("value"sv, cookie.value);
    obj.set("domain"sv, cookie.domain);
    obj.set("hostOnly"sv, cookie.host_only);
    obj.set("path"sv, cookie.path);
    obj.set("secure"sv, cookie.secure);
    obj.set("httpOnly"sv, cookie.http_only);
    obj.set("session"sv, !cookie.persistent);
    if (cookie.persistent)
        obj.set("expirationDate"sv, static_cast<double>(cookie.expiry_time.milliseconds_since_epoch()) / 1000.0);
    StringView same_site = "unspecified"sv;
    switch (cookie.same_site) {
    case HTTP::Cookie::SameSite::None:
        same_site = "no_restriction"sv;
        break;
    case HTTP::Cookie::SameSite::Lax:
        same_site = "lax"sv;
        break;
    case HTTP::Cookie::SameSite::Strict:
        same_site = "strict"sv;
        break;
    case HTTP::Cookie::SameSite::Default:
        same_site = "unspecified"sv;
        break;
    }
    obj.set("sameSite"sv, same_site);
    obj.set("storeId"sv, store_id);
    return obj;
}

void EmbedderExtensionBridge::handle_hub_host_call(u64 token, ByteString const& extension_id, StringView function, JsonObject const& args)
{
    auto send_ok = [weak = make_weak_ptr(), token](JsonValue data) {
        if (!weak)
            return;
        JsonObject reply;
        reply.set("op"sv, "hub_host_call_reply"sv);
        reply.set("token"sv, token);
        reply.set("ok"sv, true);
        reply.set("data"sv, move(data));
        weak->send_event(reply);
    };
    auto send_err = [weak = make_weak_ptr(), token](StringView error) {
        if (!weak)
            return;
        JsonObject reply;
        reply.set("op"sv, "hub_host_call_reply"sv);
        reply.set("token"sv, token);
        reply.set("ok"sv, false);
        reply.set("error"sv, MUST(String::from_utf8(error)));
        weak->send_event(reply);
    };
    auto send_json_result = [send_ok, send_err](ErrorOr<ByteString> result) {
        if (result.is_error()) {
            auto msg = ByteString::formatted("{}", result.error());
            send_err(msg.view());
            return;
        }
        auto parsed = JsonValue::from_string(result.value());
        send_ok(parsed.is_error() ? JsonValue {} : parsed.release_value());
    };

    if (function == "host.tabs.frames"sv) {
        send_json_result(extension_tab_frames(static_cast<u64>(args.get_integer<i64>("tabId"sv).value_or(-1))));
        return;
    }
    if (function == "host.scripting.inject"sv) {
        run_extension_injection_in_tab(extension_id, args, move(send_json_result));
        return;
    }

    auto& app = Application::the();
    auto* cookie_jar = app.default_cookie_jar();

    if (function == "host.cookies.get"sv && cookie_jar) {
        auto url = URL::Parser::basic_parse(args.get_string("url"sv).value_or({}));
        auto name = args.get_string("name"sv).value_or({});
        if (!url.has_value())
            return send_err("Invalid cookie URL"sv);
        auto cookie = cookie_jar->get_named_cookie(*url, name);
        if (!cookie.has_value())
            return send_ok(JsonValue {});
        auto store_id = args.get_string("storeId"sv).value_or("0"_string);
        return send_ok(serialize_extension_cookie(*cookie, store_id));
    }

    if (function == "host.cookies.getAll"sv && cookie_jar) {
        auto store_id = args.get_string("storeId"sv).value_or("0"_string);
        auto filter_name = args.get_string("name"sv);
        auto filter_domain = args.get_string("domain"sv);
        auto filter_path = args.get_string("path"sv);
        auto filter_secure = args.get_bool("secure"sv);
        auto filter_session = args.get_bool("session"sv);
        Optional<URL::URL> filter_url;
        if (auto u = args.get_string("url"sv); u.has_value())
            filter_url = URL::Parser::basic_parse(*u);

        JsonArray out;
        auto cookies = filter_url.has_value()
            ? cookie_jar->get_all_cookies_webdriver(*filter_url)
            : cookie_jar->get_all_cookies();
        for (auto const& c : cookies) {
            if (filter_name.has_value() && c.name != *filter_name)
                continue;
            if (filter_domain.has_value() && !HTTP::Cookie::domain_matches(c.domain, *filter_domain))
                continue;
            if (filter_path.has_value() && c.path != *filter_path)
                continue;
            if (filter_secure.has_value() && c.secure != *filter_secure)
                continue;
            if (filter_session.has_value() && (!c.persistent) != *filter_session)
                continue;
            out.must_append(serialize_extension_cookie(c, store_id));
        }
        return send_ok(move(out));
    }

    if (function == "host.cookies.set"sv && cookie_jar) {
        auto url = URL::Parser::basic_parse(args.get_string("url"sv).value_or({}));
        if (!url.has_value())
            return send_err("Invalid cookie URL"sv);
        auto canonical_domain = HTTP::Cookie::canonicalize_domain(*url);
        HTTP::Cookie::Cookie cookie;
        cookie.name = args.get_string("name"sv).value_or({});
        cookie.value = args.get_string("value"sv).value_or({});
        auto explicit_domain = args.get_string("domain"sv);
        cookie.host_only = !explicit_domain.has_value();
        cookie.domain = explicit_domain.value_or(canonical_domain.value_or({}));
        cookie.path = args.get_string("path"sv).value_or(HTTP::Cookie::default_path(*url));
        cookie.secure = args.get_bool("secure"sv).value_or(false);
        cookie.http_only = args.get_bool("httpOnly"sv).value_or(false);
        auto now = UnixDateTime::now();
        cookie.creation_time = now;
        cookie.last_access_time = now;
        if (auto exp = args.get_double_with_precision_loss("expirationDate"sv); exp.has_value()) {
            cookie.persistent = true;
            cookie.expiry_time = UnixDateTime::from_milliseconds_since_epoch(static_cast<i64>(*exp * 1000.0));
        } else {
            cookie.persistent = false;
            cookie.expiry_time = UnixDateTime::from_seconds_since_epoch(now.seconds_since_epoch() + 86400 * 365);
        }
        auto ss = args.get_string("sameSite"sv).value_or({});
        if (ss == "no_restriction"sv)
            cookie.same_site = HTTP::Cookie::SameSite::None;
        else if (ss == "lax"sv)
            cookie.same_site = HTTP::Cookie::SameSite::Lax;
        else if (ss == "strict"sv)
            cookie.same_site = HTTP::Cookie::SameSite::Strict;
        else
            cookie.same_site = HTTP::Cookie::SameSite::Default;
        cookie_jar->update_cookie(cookie);
        auto store_id = args.get_string("storeId"sv).value_or("0"_string);
        return send_ok(serialize_extension_cookie(cookie, store_id));
    }

    if (function == "host.cookies.remove"sv && cookie_jar) {
        auto url = URL::Parser::basic_parse(args.get_string("url"sv).value_or({}));
        auto name = args.get_string("name"sv).value_or({});
        if (!url.has_value())
            return send_err("Invalid cookie URL"sv);
        auto existing = cookie_jar->get_named_cookie(*url, name);
        if (!existing.has_value())
            return send_ok(JsonValue {});
        CookieStorageKey key {
            .name = existing->name,
            .domain = existing->domain,
            .host_only = existing->host_only,
            .path = existing->path,
            .partition_key = existing->partition_key,
        };
        cookie_jar->delete_cookie(key);
        JsonObject removed;
        removed.set("url"sv, args.get_string("url"sv).value_or({}));
        removed.set("name"sv, name);
        removed.set("storeId"sv, args.get_string("storeId"sv).value_or("0"_string));
        return send_ok(move(removed));
    }

    if (function == "host.browsingData.remove"sv) {
        Application::ClearBrowsingDataOptions options;
        if (auto opt_obj = args.get_object("options"sv); opt_obj.has_value()) {
            if (auto since = opt_obj->get_double_with_precision_loss("since"sv); since.has_value())
                options.since = UnixDateTime::from_milliseconds_since_epoch(static_cast<i64>(*since));
        }
        if (auto types = args.get_object("dataToRemove"sv); types.has_value()) {
            if (types->get_bool("cache"sv).value_or(false))
                options.delete_cached_files = Application::ClearBrowsingDataOptions::Delete::Yes;
            if (types->get_bool("history"sv).value_or(false))
                options.delete_history = Application::ClearBrowsingDataOptions::Delete::Yes;
            if (types->get_bool("downloads"sv).value_or(false))
                options.delete_download_history = Application::ClearBrowsingDataOptions::Delete::Yes;
            if (types->get_bool("cookies"sv).value_or(false) || types->get_bool("localStorage"sv).value_or(false) || types->get_bool("indexedDB"sv).value_or(false))
                options.delete_site_data = Application::ClearBrowsingDataOptions::Delete::Yes;
        }
        auto promise = app.clear_browsing_data(options);
        promise->when_resolved([send_ok = move(send_ok)](Empty) mutable {
            send_ok(JsonValue {});
        });
        return;
    }

    if (function == "host.search.engines"sv) {
        auto const& configured = Application::settings().search_engine_settings();
        JsonArray engines;
        for (auto const& eng : builtin_search_engines()) {
            JsonObject o;
            o.set("name"sv, eng.name);
            o.set("isDefault"sv, configured.engine.has_value() && configured.engine->name == eng.name);
            engines.must_append(move(o));
        }
        return send_ok(move(engines));
    }

    send_err("Unhandled Hub host call"sv);
}

void EmbedderExtensionBridge::handle_download_url(u64 token, URL::URL url, u32 redirects)
{
    auto send_err = [weak = make_weak_ptr(), token](StringView message) {
        if (!weak)
            return;
        JsonObject reply;
        reply.set("op"sv, "download_reply"sv);
        reply.set("token"sv, token);
        reply.set("ok"sv, false);
        reply.set("error"sv, MUST(String::from_utf8(message)));
        weak->send_event(reply);
    };

    if (url.scheme() != "https"sv)
        return send_err("Store packages are only downloaded over HTTPS"sv);

    auto client = Application::request_server_client();
    if (client.is_error())
        return send_err("Cannot reach the network service"sv);

    auto headers = HTTP::HeaderList::create();
    headers->set({ "User-Agent"sv, Web::default_user_agent });
    auto request = client.value()->start_request("GET"sv, url, HTTP::Cookie::SameSiteContext::same_site(), *headers, {}, HTTP::CacheMode::NoStore, HTTP::Cookie::IncludeCredentials::No);
    if (!request)
        return send_err("Cannot start the download"sv);

    auto entry = make<PendingDownload>();
    entry->request = request;
    entry->timeout = Core::Timer::create_single_shot(DOWNLOAD_TIMEOUT, [weak = make_weak_ptr(), token, send_err] {
        if (!weak)
            return;
        if (auto item = weak->m_downloads.get(token); item.has_value())
            (*item)->request->stop();
        Core::deferred_invoke([weak, token] {
            if (weak)
                weak->m_downloads.remove(token);
        });
        send_err("The download timed out"sv);
    });
    entry->timeout->start();
    m_downloads.set(token, move(entry));

    request->set_buffered_request_finished_callback(
        [weak = make_weak_ptr(), token, url, redirects, send_err](u64, Requests::RequestTimingInfo const&, Optional<Requests::NetworkError> const& network_error, NonnullRefPtr<HTTP::HeaderList> response_headers, Optional<u32> response_code, Optional<String>, Requests::CacheState, Core::ImmutableBytes payload) mutable {
            if (!weak)
                return;
            if (auto item = weak->m_downloads.get(token); item.has_value())
                (*item)->timeout->stop();
            Core::deferred_invoke([weak, token] {
                if (weak)
                    weak->m_downloads.remove(token);
            });

            if (network_error.has_value())
                return send_err("Network error while downloading the extension"sv);
            auto status = response_code.value_or(0);
            if (Web::Fetch::Infrastructure::is_redirect_status(status)) {
                auto location = response_headers->get("Location"sv);
                auto next = location.has_value() ? URL::Parser::basic_parse(*location, url) : Optional<URL::URL> {};
                if (!next.has_value() || redirects >= MAX_DOWNLOAD_REDIRECTS)
                    return send_err("The store redirected the download badly"sv);
                weak->handle_download_url(token, next.release_value(), redirects + 1);
                return;
            }
            if (status < 200 || status >= 300)
                return send_err("The store did not deliver the extension"sv);
            if (payload.size() > MAX_STORE_PACKAGE_BYTES)
                return send_err("The extension package is too large"sv);

            JsonObject reply;
            reply.set("op"sv, "download_reply"sv);
            reply.set("token"sv, token);
            reply.set("ok"sv, true);
            weak->send_event(reply, payload.bytes());
        });
}

}
