/*
 * Copyright (c) 2026, cgx-webext contributors
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

#include <AK/JsonObject.h>
#include <AK/JsonValue.h>
#include <AK/StringBuilder.h>
#include <AK/Time.h>
#include <LibWebView/Extensions/ExtensionManager.h>

#include <webext.h>

namespace WebView {

struct ExtensionCallbackContext {
    AK_ALLOC_WITH_KMALLOC;

    explicit ExtensionCallbackContext(ExtensionManager::Delegate& delegate)
        : delegate(delegate)
    {
    }

    ExtensionManager::Delegate& delegate;
    WebExtHandle* handle { nullptr };
};

namespace {

using Context = ExtensionCallbackContext;

WebExtStr to_c(StringView view)
{
    return { reinterpret_cast<u8 const*>(view.characters_without_null_termination()), view.length() };
}

StringView to_view(WebExtStr str)
{
    return { reinterpret_cast<char const*>(str.ptr), str.len };
}

// Copies a library-owned string into an AK string and releases it.
Optional<ByteString> take(WebExtString string)
{
    if (!string.ptr)
        return {};
    ByteString result { reinterpret_cast<char const*>(string.ptr), string.len };
    webext_string_free(string);
    return result;
}

// The C callbacks recover the Delegate from user_data. The trampolines below are only ever called on the thread that owns
// the manager.
WebExtString make_library_string(StringView view)
{
    return webext_string_from(reinterpret_cast<u8 const*>(view.characters_without_null_termination()), view.length());
}

WebExtString read_storage(void* user, WebExtStr id, WebExtStr key)
{
    auto value = static_cast<Context*>(user)->delegate.read_extension_data(to_view(id), to_view(key));
    if (!value.has_value())
        return { nullptr, 0, 0 };
    return make_library_string(value->view());
}

void write_storage(void* user, WebExtStr id, WebExtStr key, WebExtStr json)
{
    static_cast<Context*>(user)->delegate.write_extension_data(to_view(id), to_view(key), to_view(json));
}

i64 now_ms(void* user)
{
    return static_cast<Context*>(user)->delegate.now_ms();
}

WebExtString ui_locale(void* user)
{
    auto locale = static_cast<Context*>(user)->delegate.ui_locale();
    return make_library_string(locale.bytes_as_string_view());
}

void dispatch_event(void* user, WebExtStr id, WebExtStr event, WebExtStr args)
{
    static_cast<Context*>(user)->delegate.dispatch_event(to_view(id), to_view(event), to_view(args));
}

void deliver_to_context(void* user, u64 context_id, WebExtStr envelope)
{
    static_cast<Context*>(user)->delegate.deliver_to_context(context_id, to_view(envelope));
}

WebExtString describe_tab(void* user, i64 tab_id)
{
    auto tab = static_cast<Context*>(user)->delegate.describe_tab(tab_id);
    if (!tab.has_value())
        return { nullptr, 0, 0 };
    return make_library_string(tab->view());
}

void call_browser(void* user, WebExtStr call, u64 token)
{
    auto* context = static_cast<Context*>(user);
    context->delegate.call_browser(to_view(call), [context, token](ErrorOr<ByteString> result) {
        if (result.is_error()) {
            auto message = ByteString::formatted("{}", result.error());
            webext_complete(context->handle, token, false, to_c(message.view()));
        } else {
            webext_complete(context->handle, token, true, to_c(result.value().view()));
        }
    });
}

void request_permissions(void* user, WebExtStr id, WebExtStr request, u64 token)
{
    auto* context = static_cast<Context*>(user);
    context->delegate.request_permissions(to_view(id), to_view(request), [context, token](bool granted) {
        webext_complete_permission(context->handle, token, granted);
    });
}

void schedule_alarm_check(void* user, i64 at_ms)
{
    static_cast<Context*>(user)->delegate.schedule_alarm_check(at_ms);
}

}

i64 ExtensionManager::Delegate::now_ms()
{
    return UnixDateTime::now().milliseconds_since_epoch();
}

NonnullOwnPtr<ExtensionManager> ExtensionManager::create(Delegate& delegate)
{
    return adopt_own(*new ExtensionManager(delegate));
}

ExtensionManager::ExtensionManager(Delegate& delegate)
    : m_context(make<ExtensionCallbackContext>(delegate))
{
    VERIFY(webext_abi_version() == WEBEXT_ABI_VERSION);

    WebExtHostCallbacks callbacks {
        .user_data = m_context.ptr(),
        .read_storage = read_storage,
        .write_storage = write_storage,
        .now_ms = now_ms,
        .ui_locale = ui_locale,
        .dispatch_event = dispatch_event,
        .deliver_to_context = deliver_to_context,
        .describe_tab = describe_tab,
        .call_browser = call_browser,
        .request_permissions = request_permissions,
        .schedule_alarm_check = schedule_alarm_check,
    };
    m_handle = webext_create(&callbacks);
    VERIFY(m_handle);
    m_context->handle = m_handle;
}

ExtensionManager::~ExtensionManager()
{
    webext_destroy(m_handle);
}

static ErrorOr<Variant<ExtensionManager::InstalledExtension, ExtensionManager::InstallFailure>> parse_install_result(Optional<ByteString> raw)
{
    using InstalledExtension = ExtensionManager::InstalledExtension;
    using InstallFailure = ExtensionManager::InstallFailure;

    if (!raw.has_value())
        return Error::from_string_literal("The extension core returned no result");

    auto json = TRY(JsonValue::from_string(raw->view()));
    auto const& object = json.as_object();
    if (!object.get_bool("ok"sv).value_or(false)) {
        StringBuilder builder;
        if (auto errors = object.get_array("errors"sv); errors.has_value()) {
            errors->for_each([&](JsonValue const& error) {
                if (!error.is_object())
                    return;
                if (!builder.is_empty())
                    builder.append("; "sv);
                if (auto field = error.as_object().get_string("field"sv); field.has_value() && !field->is_empty())
                    builder.appendff("{}: ", *field);
                builder.append(error.as_object().get_string("message"sv).value_or({}));
            });
        }
        return Variant<InstalledExtension, InstallFailure> { InstallFailure { TRY(builder.to_string()) } };
    }

    return Variant<InstalledExtension, InstallFailure> { InstalledExtension {
        .id = object.get_string("id"sv).value_or({}),
        .display_name = object.get_string("displayName"sv).value_or({}),
        .version = object.get_string("version"sv).value_or({}),
        .base_url = object.get_string("baseUrl"sv).value_or({}),
        .result_json = raw.release_value(),
    } };
}

ErrorOr<Variant<ExtensionManager::InstalledExtension, ExtensionManager::InstallFailure>> ExtensionManager::install_package(ReadonlyBytes package, StringView id_override, StringView base_url)
{
    return parse_install_result(take(webext_install_package(m_handle, package.data(), package.size(), to_c(id_override), to_c(base_url))));
}

ErrorOr<Variant<ExtensionManager::InstalledExtension, ExtensionManager::InstallFailure>> ExtensionManager::install_directory(StringView path, StringView id_override, StringView base_url)
{
    return parse_install_result(take(webext_install_directory(m_handle, to_c(path), to_c(id_override), to_c(base_url))));
}

Optional<ExtensionManager::ServedResource> ExtensionManager::serve(StringView url, StringView initiator)
{
    WebExtString body { nullptr, 0, 0 };
    auto meta = take(webext_serve(m_handle, to_c(url), to_c(initiator), &body));
    if (!meta.has_value())
        return {};

    auto json = JsonValue::from_string(meta->view());
    if (json.is_error() || !json.value().is_object()) {
        webext_string_free(body);
        return {};
    }
    auto const& object = json.value().as_object();

    ServedResource resource;
    resource.status = object.get_u16("status"sv).value_or(500);
    resource.mime_type = object.get_string("mime"sv).value_or({}).to_byte_string();
    if (auto csp = object.get_string("csp"sv); csp.has_value())
        resource.content_security_policy = csp->to_byte_string();
    resource.extension_id = object.get_string("extensionId"sv).value_or({}).to_byte_string();
    if (body.ptr) {
        auto copy = ByteBuffer::copy(body.ptr, body.len);
        webext_string_free(body);
        if (copy.is_error())
            return {};
        resource.body = copy.release_value();
    }
    return resource;
}

ByteString ExtensionManager::content_script_bundles(StringView url, bool is_top_frame, StringView fallback_url)
{
    return take(webext_content_script_bundles(m_handle, to_c(url), is_top_frame, to_c(fallback_url))).value_or("[]"sv);
}

ByteString ExtensionManager::content_script_programs(StringView url, bool is_top_frame, StringView fallback_url)
{
    return take(webext_content_script_programs(m_handle, to_c(url), is_top_frame, to_c(fallback_url))).value_or("[]"sv);
}

Optional<ByteString> ExtensionManager::background_url(StringView extension_id)
{
    return take(webext_background_url(m_handle, to_c(extension_id)));
}

bool ExtensionManager::is_background_url(StringView extension_id, StringView url)
{
    return webext_is_background_url(m_handle, to_c(extension_id), to_c(url));
}

Optional<ByteString> ExtensionManager::extension_for_url(StringView url)
{
    return take(webext_extension_for_url(m_handle, to_c(url)));
}

bool ExtensionManager::uninstall(StringView extension_id)
{
    return webext_uninstall(m_handle, to_c(extension_id));
}

void ExtensionManager::dispatch(StringView call_json, Function<void(bool, StringView)> on_reply)
{
    // The reply is invoked exactly once, so the heap copy handed to C is reclaimed there.
    auto* callback = new Function<void(bool, StringView)>(move(on_reply));
    webext_dispatch(
        m_handle, to_c(call_json), [](void* user, bool ok, WebExtStr json) {
            auto owned = adopt_own(*static_cast<Function<void(bool, StringView)>*>(user));
            (*owned)(ok, to_view(json));
        },
        callback);
}

ErrorOr<bool> ExtensionManager::context_created(ContextDescription const& context)
{
    StringView kind;
    switch (context.kind) {
    case ContextKind::Background:
        kind = "background"sv;
        break;
    case ContextKind::ExtensionPage:
        kind = "extension_page"sv;
        break;
    case ContextKind::ContentScript:
        kind = "content_script"sv;
        break;
    case ContextKind::Other:
        kind = "other"sv;
        break;
    }

    JsonObject json;
    json.set("id"sv, context.id);
    json.set("extensionId"sv, TRY(String::from_utf8(context.extension_id)));
    json.set("kind"sv, TRY(String::from_utf8(kind)));
    if (context.tab_id.has_value())
        json.set("tabId"sv, *context.tab_id);
    if (context.frame_id.has_value())
        json.set("frameId"sv, *context.frame_id);
    if (!context.url.is_empty())
        json.set("url"sv, TRY(String::from_utf8(context.url)));

    auto serialized = json.serialized();
    return webext_context_created(m_handle, to_c(serialized.bytes_as_string_view()));
}

void ExtensionManager::context_destroyed(u64 context_id)
{
    webext_context_destroyed(m_handle, context_id);
}

Optional<ByteString> ExtensionManager::init_data_for_context(u64 context_id)
{
    return take(webext_init_data_for_context(m_handle, context_id));
}

Optional<ByteString> ExtensionManager::extension_info(StringView extension_id)
{
    return take(webext_extension_info(m_handle, to_c(extension_id)));
}

Optional<ByteString> ExtensionManager::init_data(StringView extension_id)
{
    return take(webext_init_data(m_handle, to_c(extension_id)));
}

Optional<ByteString> ExtensionManager::content_scripts_for(StringView extension_id, StringView url, bool is_top_frame, StringView fallback_url)
{
    return take(webext_content_scripts(m_handle, to_c(extension_id), to_c(url), is_top_frame, to_c(fallback_url)));
}

bool ExtensionManager::is_web_accessible(StringView extension_id, StringView path, Initiator initiator, StringView initiator_value)
{
    return webext_is_web_accessible(m_handle, to_c(extension_id), to_c(path), static_cast<u32>(initiator), to_c(initiator_value));
}

Optional<ByteBuffer> ExtensionManager::read_file(StringView extension_id, StringView path)
{
    WebExtString file { nullptr, 0, 0 };
    if (!webext_read_file(m_handle, to_c(extension_id), to_c(path), &file))
        return {};
    auto buffer = ByteBuffer::copy(file.ptr, file.len);
    webext_string_free(file);
    if (buffer.is_error())
        return {};
    return buffer.release_value();
}

void ExtensionManager::tick()
{
    webext_tick(m_handle);
}

void ExtensionManager::notify_installed(StringView extension_id, StringView reason, StringView previous_version)
{
    webext_notify_installed(m_handle, to_c(extension_id), to_c(reason), to_c(previous_version));
}

void ExtensionManager::notify_startup(StringView extension_id)
{
    webext_notify_startup(m_handle, to_c(extension_id));
}

}
