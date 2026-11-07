/*
 * Copyright (c) 2026, cgx-webext contributors
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

#if defined(ENABLE_WEBEXT)

#    include <AK/CharacterTypes.h>
#    include <AK/JsonArray.h>
#    include <AK/JsonObject.h>
#    include <AK/JsonValue.h>
#    include <AK/LexicalPath.h>
#    include <AK/StringBuilder.h>
#    include <LibFileSystem/FileSystem.h>
#    include <LibWebView/Extensions/ApplicationExtensionDelegate.h>
#    include <LibWebView/Utilities.h>
#    include <LibWebView/WebContentPage.h>

namespace WebView {

// Extension ids and storage keys come from the extension, so they must not be able to name another path.
static ByteString sanitize_path_component(StringView component)
{
    StringBuilder builder;
    for (auto character : component)
        builder.append(is_ascii_alphanumeric(character) || character == '.' || character == '-' || character == '_' ? character : '_');
    auto sanitized = builder.to_byte_string();
    if (sanitized.is_empty() || sanitized == "."sv || sanitized == ".."sv)
        return "_"sv;
    return sanitized;
}

ApplicationExtensionDelegate::ApplicationExtensionDelegate(ByteString data_directory)
    : m_data_directory(move(data_directory))
{
}

ByteString ApplicationExtensionDelegate::path_for(StringView extension_id, StringView key) const
{
    return LexicalPath::join(m_data_directory, sanitize_path_component(extension_id), ByteString::formatted("{}.json", sanitize_path_component(key))).string();
}

Optional<ByteString> ApplicationExtensionDelegate::read_extension_data(StringView extension_id, StringView key)
{
    auto path = path_for(extension_id, key);
    if (!FileSystem::exists(path))
        return {};

    auto json = read_json_file(path);
    if (json.is_error()) {
        warnln("Unable to read extension data '{}': {}", path, json.error());
        return {};
    }
    return json.value().serialized().to_byte_string();
}

void ApplicationExtensionDelegate::write_extension_data(StringView extension_id, StringView key, StringView json)
{
    auto value = JsonValue::from_string(json);
    if (value.is_error()) {
        warnln("Refusing to persist invalid JSON for extension data '{}'/'{}'", extension_id, key);
        return;
    }

    auto path = path_for(extension_id, key);
    if (auto result = write_json_file(path, value.value()); result.is_error())
        warnln("Unable to persist extension data '{}': {}", path, result.error());
}

void ApplicationExtensionDelegate::remove_extension_data(StringView extension_id)
{
    auto directory = LexicalPath::join(m_data_directory, sanitize_path_component(extension_id)).string();
    if (!FileSystem::is_directory(directory))
        return;
    if (auto result = FileSystem::remove(directory, FileSystem::RecursionMode::Allowed); result.is_error())
        warnln("Unable to remove extension data '{}': {}", directory, result.error());
}

void ApplicationExtensionDelegate::dispatch_event(StringView extension_id, StringView event, StringView args_json)
{
    JsonObject message;
    message.set("kind"sv, "event"sv);
    message.set("name"sv, MUST(String::from_utf8(event)));
    auto args = JsonValue::from_string(args_json);
    message.set("args"sv, args.is_error() ? JsonValue { JsonArray {} } : args.release_value());
    deliver_event_to_extension(extension_id, message.serialized());
}

void ApplicationExtensionDelegate::deliver_to_context(u64 context_id, StringView envelope_json)
{
    // Envelopes from the message bus (ports, one-shot messages) for Shim/boot.js.
    auto message = ByteString::formatted("{{\"kind\":\"envelope\",\"envelope\":{}}}", envelope_json);
    deliver_to_extension_context(context_id, message);
}

void ApplicationExtensionDelegate::call_browser(StringView call_json, Function<void(ErrorOr<ByteString>)> reply)
{
    // TODO: tabs.*, windows.*, action.*, cookies.*, scripting.*, contextMenus.*.
    dbgln_if(false, "extension: unimplemented browser call {}", call_json);
    reply(Error::from_string_literal("This browser extension API is not implemented yet"));
}

void ApplicationExtensionDelegate::request_permissions(StringView, StringView, Function<void(bool)> reply)
{
    // TODO: Ask the user. Until there is UI, optional permissions are refused.
    reply(false);
}

void ApplicationExtensionDelegate::schedule_alarm_check(i64 at_ms)
{
    auto delay = max<i64>(0, at_ms - now_ms());
    m_alarm_timer = Core::Timer::create_single_shot(AK::Duration::from_milliseconds(delay), [this] {
        if (m_manager)
            m_manager->tick();
    });
    m_alarm_timer->start();
}

}

#endif
