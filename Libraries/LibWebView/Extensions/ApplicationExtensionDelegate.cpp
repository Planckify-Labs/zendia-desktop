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
#    include <LibCore/EventLoop.h>
#    include <LibWebView/Application.h>
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
    // Not for the extension: the core asks the browser UI to redraw its toolbar button, menus or shortcuts.
    if (event.starts_with("__ui."sv)) {
        auto args = JsonValue::from_string(args_json);
        StringView what;
        String what_storage;
        if (!args.is_error() && args.value().is_array() && args.value().as_array().size() > 0 && args.value().as_array().at(0).is_string()) {
            what_storage = args.value().as_array().at(0).as_string();
            what = what_storage.bytes_as_string_view();
        }
        Application::the().extension_ui_changed(extension_id, what);
        return;
    }

    // Which stages of a request extensions care about changed; the renderers have to know.
    if (event == "__network.changed"sv) {
        Application::the().extension_network_interest_changed();
        return;
    }

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
    auto parsed = JsonValue::from_string(call_json);
    if (parsed.is_error() || !parsed.value().is_object()) {
        reply(Error::from_string_literal("Malformed browser call"));
        return;
    }
    auto const& call = parsed.value().as_object();
    auto function = call.get_string("function"sv).value_or({}).to_byte_string();
    auto extension_id = call.get_string("extensionId"sv).value_or({}).to_byte_string();
    JsonObject args;
    if (auto list = call.get_array("args"sv); list.has_value() && list->size() > 0 && list->at(0).is_object())
        args = list->at(0).as_object();

    // Primitives the core asks of the browser (see crates/webext/src/browser_api.rs). Frames and injection work on pages,
    // which any UI has; the rest needs windows and tabs, which the UI provides.
    if (function == "host.tabs.frames"sv) {
        reply(extension_tab_frames(static_cast<u64>(args.get_integer<i64>("tabId"sv).value_or(-1))));
        return;
    }
    if (function == "host.scripting.inject"sv) {
        run_extension_injection_in_tab(extension_id, args, move(reply));
        return;
    }
    if (function == "host.runtime.reload"sv) {
        // Reloading stops the context that asked; answer first.
        reply(ByteString { "null"sv });
        Core::deferred_invoke([extension_id] { Application::the().reload_extension(extension_id); });
        return;
    }
    if (function.starts_with("host."sv)) {
        Application::the().extension_host_call(extension_id, function, args, move(reply));
        return;
    }

    // Requirement: the error names the API that is missing.
    auto message = ByteString::formatted("{} is not implemented yet in Ladybird", function);
    reply(Error::from_string_view(message.view()));
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
