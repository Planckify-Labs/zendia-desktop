/*
 * Copyright (c) 2026, cgx-webext contributors
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

#include <AK/JsonArray.h>
#include <AK/JsonObject.h>
#include <AK/JsonValue.h>
#include <LibGC/Heap.h>
#include <LibJS/Runtime/AbstractOperations.h>
#include <LibJS/Runtime/Error.h>
#include <LibJS/Runtime/PrimitiveString.h>
#include <LibURL/Origin.h>
#include <LibWeb/Bindings/PlatformObject.h>
#include <LibWeb/Bindings/PrincipalHostDefined.h>
#include <LibWeb/Bindings/Wrappable.h>
#include <LibWeb/Bindings/WrapperWorld.h>
#include <LibWeb/DOM/Document.h>
#include <LibWeb/HTML/Scripting/ClassicScript.h>
#include <LibWeb/HTML/Scripting/Environments.h>
#include <LibWeb/HTML/Scripting/TemporaryExecutionContext.h>
#include <LibWeb/HTML/BrowsingContext.h>
#include <LibWeb/HTML/Window.h>
#include <LibWeb/Internals/ExtensionBridge.h>
#include <LibWeb/Internals/ExtensionWorld.h>
#include <LibWeb/Page/Page.h>
#include <LibWeb/WebIDL/AbstractOperations.h>

namespace Web::Internals {

GC_DEFINE_ALLOCATOR(ExtensionBridge);

GC::Ref<ExtensionBridge> ExtensionBridge::create(HTML::Window& window, String extension_id)
{
    return create(window, move(extension_id), Kind::Page, nullptr);
}

GC::Ref<ExtensionBridge> ExtensionBridge::create(HTML::Window& window, String extension_id, Kind kind, GC::Ptr<ExtensionWorld> world)
{
    return GC::Heap::the().allocate<ExtensionBridge>(window, move(extension_id), kind, world);
}

ExtensionBridge::ExtensionBridge(HTML::Window& window, String extension_id, Kind kind, GC::Ptr<ExtensionWorld> world)
    : InternalsBase(window)
    , m_extension_id(move(extension_id))
    , m_kind(kind)
    , m_world(world)
{
}

ExtensionBridge::~ExtensionBridge() = default;

void ExtensionBridge::visit_edges(GC::Cell::Visitor& visitor)
{
    Base::visit_edges(visitor);
    visitor.visit(m_world);
    visitor.visit(m_message_handler);
    visitor.visit(m_stage_runner);
    for (auto& [id, promise] : m_pending_calls)
        visitor.visit(promise);
}

bool ExtensionBridge::is_extension_origin(URL::Origin const& origin)
{
    if (origin.is_opaque())
        return false;
    auto const& scheme = origin.scheme();
    return scheme.has_value() && (*scheme == "chrome-extension"sv || *scheme == "moz-extension"sv);
}

void ExtensionBridge::install_for_extension_page(JS::Realm& realm, HTML::Window& window)
{
    static auto const property_name = "__cgxBridge"_utf16_fly_string;
    auto bridge = create(window, String {});
    window.associated_document().set_extension_bridge(String {}, bridge);
    realm.global_object().define_direct_property(property_name, Bindings::wrap(Bindings::host_defined_wrapper_world(realm), realm, bridge), JS::Attribute::Configurable | JS::Attribute::Writable);
}

void ExtensionBridge::inject_content_scripts(DOM::Document& document)
{
    // Extension pages have their own bridge; only web documents (and local files) get content scripts.
    auto const& scheme = document.url().scheme();
    if (!scheme.is_one_of("http"sv, "https"sv, "file"sv) || is_extension_origin(document.origin()))
        return;
    auto window = document.window();
    if (!window)
        return;

    auto programs_json = document.page().client().page_did_request_content_script_programs(document.id());
    if (!programs_json.has_value() || programs_json->is_empty() || *programs_json == "[]"sv)
        return;
    auto programs = JsonValue::from_string(*programs_json);
    if (programs.is_error() || !programs.value().is_array())
        return;

    programs.value().as_array().for_each([&](JsonValue const& entry) {
        if (!entry.is_object())
            return;
        auto extension_id = entry.as_object().get_string("extensionId"sv);
        auto program = entry.as_object().get_string("program"sv);
        if (!extension_id.has_value() || !program.has_value())
            return;
        // Which world a program runs in is the core's decision (the manifest's `world`). The default is the isolated one.
        auto world = entry.as_object().get_string("world"sv).value_or("ISOLATED"_string);
        auto kind = world == "MAIN"sv ? Kind::MainScripts : world == "MAIN_BRIDGE"sv ? Kind::StorePage
                                                                                    : Kind::ContentScripts;
        if (document.extension_bridge(document_key(*extension_id, kind)))
            return;
        (void)start_program(document, *extension_id, *program, kind);
    });
}

String ExtensionBridge::document_key(String const& extension_id, Kind kind)
{
    // An extension's scripts of the page's world are kept apart from its isolated world's. A key with a slash is no
    // extension id, so the browser process cannot address it.
    if (kind == Kind::MainScripts)
        return MUST(String::formatted("main/{}", extension_id));
    return extension_id;
}

GC::Ptr<ExtensionBridge> ExtensionBridge::start_program(DOM::Document& document, String const& extension_id, String const& program, Kind kind)
{
    auto window = document.window();
    if (!window)
        return {};

    // Content scripts get a world of their own in this document, in which all the extension's scripts run. The programs
    // for the page's world run in the page's own realm.
    GC::Ptr<ExtensionWorld> world;
    if (kind == Kind::ContentScripts) {
        if (!document.browsing_context())
            return {};
        world = ExtensionWorld::create(*window, extension_id);
    }

    auto bridge = create(*window, extension_id, kind, world);
    document.set_extension_bridge(document_key(extension_id, kind), bridge);

    auto& settings = world ? world->settings() : document.relevant_settings_object();
    auto& realm = settings.realm();
    auto& vm = realm.vm();

    // The program is a function expression; evaluating it yields the function to call. A classic script is not subject to
    // the page's CSP, which governs the page's own scripts. Errors are not reported to the page: they are the extension's.
    auto filename = ByteString::formatted("extension://{}/content-scripts", extension_id);
    auto script = HTML::ClassicScript::create(filename, Utf16String::from_utf8(program), settings, document.url());
    auto completion = script->run(HTML::ClassicScript::RethrowErrors::Yes);
    if (completion.is_error() || !completion.value().is_function()) {
        dbgln("Content scripts of extension {} could not be set up", extension_id);
        return bridge;
    }

    HTML::TemporaryExecutionContext execution_context { settings, HTML::TemporaryExecutionContext::CallbacksEnabled::Yes };
    if (kind == Kind::MainScripts) {
        // No bridge: these scripts are the page's own as far as the page can tell, and the API is not theirs.
        auto result = JS::call(vm, completion.value().as_function(), JS::js_undefined());
        if (result.is_error())
            dbgln("Page-world content scripts of extension {} threw while starting", extension_id);
        else if (auto runner = result.release_value(); runner.is_function())
            bridge->m_stage_runner = as<JS::FunctionObject>(runner.as_object());
        return bridge;
    }

    auto bridge_value = Bindings::wrap(Bindings::host_defined_wrapper_world(realm), realm, bridge);
    auto result = JS::call(vm, completion.value().as_function(), JS::js_undefined(), bridge_value);
    if (result.is_error())
        dbgln("Content scripts of extension {} threw while starting", extension_id);
    return bridge;
}

static bool stage_reached(DOM::Document const& document, StringView run_at)
{
    if (run_at == "document_start"sv)
        return true;
    if (run_at == "document_end"sv)
        return document.readiness() != HTML::DocumentReadyState::Loading;
    return document.readiness() == HTML::DocumentReadyState::Complete;
}

void ExtensionBridge::run_injection(DOM::Document& document, String const& extension_id, String const& setup_program, Injection injection)
{
    if (is_extension_origin(document.origin()) || extension_id.is_empty())
        return;
    auto bridge = document.extension_bridge(document_key(extension_id, Kind::ContentScripts));
    if (!bridge) {
        bridge = start_program(document, extension_id, setup_program, Kind::ContentScripts);
        if (!bridge)
            return;
    }
    if (!bridge->m_world)
        return;
    if (!stage_reached(document, injection.run_at)) {
        bridge->m_deferred_injections.append(move(injection));
        return;
    }
    bridge->execute(injection);
}

void ExtensionBridge::run_due_injections()
{
    auto& document = window().associated_document();
    Vector<Injection> due;
    m_deferred_injections.remove_all_matching([&](Injection const& injection) {
        if (!stage_reached(document, injection.run_at))
            return false;
        due.append(injection);
        return true;
    });
    for (auto const& injection : due)
        execute(injection);
}

void ExtensionBridge::execute(Injection const& injection)
{
    if (!m_world || m_world->is_detached())
        return;
    auto& document = window().associated_document();
    auto& world = *m_world;
    auto& settings = world.settings();
    auto& realm = settings.realm();
    auto& vm = realm.vm();
    HTML::TemporaryExecutionContext execution_context { settings, HTML::TemporaryExecutionContext::CallbacksEnabled::Yes };

    // The content-script program put its hooks on the bridge's wrapper, which only this closure and the browser can reach.
    auto wrapper = Bindings::wrap(world.wrapper_world(), realm, GC::Ref<ExtensionBridge> { *this });
    JS::Object& hooks = *wrapper;
    auto hook = [&](Utf16FlyString const& name) -> JS::Value {
        auto value = hooks.get(name);
        return value.is_error() ? JS::js_undefined() : value.release_value();
    };
    auto report = hook("__cgxReport"_utf16_fly_string);
    if (!report.is_function()) {
        dbgln("Extension {} has no content-script context in {}; cannot inject", m_extension_id, document.url());
        return;
    }
    auto token = JS::Value(static_cast<double>(injection.token));

    if (injection.kind == "js"sv) {
        // Code for the page's world gets nothing from the extension; code for the extension's world finds its API under
        // the binding, which it deletes first thing.
        bool main_world = injection.world == "MAIN"sv;
        Utf16FlyString binding = Utf16FlyString::from_utf8(injection.binding);
        if (!main_world)
            realm.global_object().define_direct_property(binding, hook("__cgxApi"_utf16_fly_string), JS::Attribute::Configurable | JS::Attribute::Writable);
        auto filename = ByteString::formatted("extension://{}/injected", m_extension_id);
        auto script = HTML::ClassicScript::create(filename, Utf16String::from_utf8(injection.code), main_world ? document.relevant_settings_object() : settings, document.url());
        auto completion = script->run(HTML::ClassicScript::RethrowErrors::Yes);
        // The code deletes the binding first thing; a script that did not parse never ran, so delete it here too.
        if (!main_world)
            (void)realm.global_object().internal_delete(binding);
        JS::Value arguments[3] = { token, JS::js_undefined(), JS::js_undefined() };
        if (completion.is_error())
            arguments[2] = completion.value().is_undefined() ? JS::Value(JS::PrimitiveString::create(vm, Utf16String::from_utf8("Script failed"sv))) : completion.value();
        else
            arguments[1] = completion.value();
        (void)JS::call(vm, report.as_function(), JS::js_undefined(), arguments[0], arguments[1], arguments[2]);
        return;
    }

    auto css = hook("__cgxCss"_utf16_fly_string);
    if (!css.is_function())
        return;
    (void)JS::call(vm, css.as_function(), JS::js_undefined(), token, JS::PrimitiveString::create(vm, Utf16String::from_utf8(injection.code)), JS::Value(injection.kind == "removeCss"sv));
}

void ExtensionBridge::run_content_script_stage(DOM::Document& document, StringView stage)
{
    for (auto const& bridge : document.content_script_bridges()) {
        bridge->run_stage(stage);
        bridge->run_due_injections();
    }
}

void ExtensionBridge::run_stage(StringView stage)
{
    if (m_kind != Kind::MainScripts) {
        deliver(MUST(String::formatted("{{\"kind\":\"run\",\"stage\":\"{}\"}}", stage)));
        return;
    }
    if (!m_stage_runner)
        return;
    auto& settings = window().associated_document().relevant_settings_object();
    HTML::TemporaryExecutionContext execution_context { settings, HTML::TemporaryExecutionContext::CallbacksEnabled::Yes };
    auto& vm = settings.realm().vm();
    // An error in the extension's page-world script is the page's business, as it would be for any script of the page.
    (void)JS::call(vm, *m_stage_runner, JS::js_undefined(), JS::PrimitiveString::create(vm, Utf16String::from_utf8(stage)));
}

void ExtensionBridge::document_destroyed(DOM::Document& document)
{
    for (auto const& bridge : document.content_script_bridges()) {
        if (bridge->m_world)
            bridge->m_world->detach();
    }
}

void ExtensionBridge::install_firefox_helpers()
{
    if (m_world)
        m_world->install_firefox_helpers();
}

JS::Realm& ExtensionBridge::bridge_realm() const
{
    return m_world ? m_world->realm() : HTML::relevant_realm(window());
}

HTML::CrossProcessId ExtensionBridge::document_id() const
{
    return window().associated_document().id();
}

Utf16String ExtensionBridge::context_init()
{
    auto json = page().client().page_did_request_extension_context_init(document_id(), m_extension_id);
    return Utf16String::from_utf8(json.value_or("null"_string));
}

GC::Ref<WebIDL::Promise> ExtensionBridge::call(Utf16String const& name, Utf16String const& args_json)
{
    auto promise = WebIDL::create_promise(bridge_realm());
    auto request_id = m_next_request_id++;
    m_pending_calls.set(request_id, promise);
    page().client().page_did_call_extension_api(document_id(), m_extension_id, request_id, name.to_utf8(), args_json.to_utf8());
    return promise;
}

Utf16String ExtensionBridge::call_sync(Utf16String const& name, Utf16String const& args_json)
{
    auto result = page().client().page_did_call_extension_api_sync(document_id(), m_extension_id, name.to_utf8(), args_json.to_utf8());
    return Utf16String::from_utf8(result.value_or(String {}));
}

void ExtensionBridge::set_message_handler(WebIDL::CallbackType& handler)
{
    m_message_handler = &handler;
}

void ExtensionBridge::complete_call(u64 request_id, bool ok, String const& json)
{
    auto promise = m_pending_calls.take(request_id);
    if (!promise.has_value())
        return;

    auto& realm = (*promise)->promise()->shape().realm();
    HTML::TemporaryExecutionContext execution_context { realm, HTML::TemporaryExecutionContext::CallbacksEnabled::Yes };
    if (ok)
        WebIDL::resolve_promise(realm, *promise, JS::PrimitiveString::create(realm.vm(), Utf16String::from_utf8(json)));
    else
        WebIDL::reject_promise(realm, *promise, JS::Error::create(realm, Utf16String::from_utf8(json)));
}

void ExtensionBridge::deliver(String const& json)
{
    if (!m_message_handler)
        return;
    auto& realm = m_message_handler->associated_realm();
    HTML::TemporaryExecutionContext execution_context { realm, HTML::TemporaryExecutionContext::CallbacksEnabled::Yes };
    JS::Value argument = JS::PrimitiveString::create(realm.vm(), Utf16String::from_utf8(json));
    (void)WebIDL::invoke_callback(*m_message_handler, {}, WebIDL::ExceptionBehavior::Report, { &argument, 1 });
}

}
