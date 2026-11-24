/*
 * Copyright (c) 2026, cgx-webext contributors
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

#include <LibGC/Heap.h>
#include <LibJS/Runtime/AbstractOperations.h>
#include <LibJS/Runtime/Array.h>
#include <LibJS/Runtime/Error.h>
#include <LibJS/Runtime/GlobalEnvironment.h>
#include <LibJS/Runtime/NativeFunction.h>
#include <LibJS/Runtime/Realm.h>
#include <LibJS/Runtime/ValueInlines.h>
#include <LibWeb/Bindings/HostDefined.h>
#include <LibWeb/Bindings/Intrinsics.h>
#include <LibWeb/Bindings/MainThreadVM.h>
#include <LibWeb/Bindings/PrincipalHostDefined.h>
#include <LibWeb/Bindings/Wrappable.h>
#include <LibWeb/Bindings/WrapperWorld.h>
#include <LibWeb/DOM/Document.h>
#include <LibWeb/HTML/BrowsingContext.h>
#include <LibWeb/HTML/PolicyContainers.h>
#include <LibWeb/HTML/Scripting/Environments.h>
#include <LibWeb/HTML/StructuredSerialize.h>
#include <LibWeb/HTML/Window.h>
#include <LibWeb/HTML/WindowProxy.h>
#include <LibWeb/Internals/ExtensionWorld.h>
#include <LibWeb/WebIDL/ExceptionOrUtils.h>

namespace Web::Internals {

GC_DEFINE_ALLOCATOR(ExtensionWorld);

namespace {

// The host-defined data of an extension world's realm. Like any realm that is not a window's, it points at the window's
// realm as its principal realm, so everything that asks a realm for its settings object (the incumbent settings object, a
// callback's context, promise jobs) gets the page's. It also names the world, for the places that have a realm only.
class ExtensionHostDefined final : public Bindings::HostDefined {
    GC_CELL(ExtensionHostDefined, Bindings::HostDefined);
    GC_DECLARE_ALLOCATOR(ExtensionHostDefined);

public:
    ExtensionHostDefined(GC::Ref<Bindings::Intrinsics> intrinsics, GC::Ref<Bindings::WrapperWorld> wrapper_world, GC::Ref<JS::Realm> principal_realm)
        : HostDefined(intrinsics, wrapper_world, principal_realm)
    {
    }

    GC::Ptr<ExtensionWorld> world;

    virtual void visit_edges(Cell::Visitor& visitor) override
    {
        Base::visit_edges(visitor);
        visitor.visit(world);
    }
};

GC_DEFINE_ALLOCATOR(ExtensionHostDefined);

// The environment settings object of an extension world. A script, a module or a fetch takes its realm, its module map
// and its global object from the settings object it is created for, so the world needs one of its own to run scripts and
// load modules in its realm. Everything the page decides (origin, URL, policy container, event loop, document) is the
// page's, so that the world cannot do more or less than a script of the page could.
class ExtensionWorldSettings final : public HTML::EnvironmentSettingsObject {
    GC_CELL(ExtensionWorldSettings, HTML::EnvironmentSettingsObject);
    GC_DECLARE_ALLOCATOR(ExtensionWorldSettings);

public:
    ExtensionWorldSettings(GC::Ref<HTML::EnvironmentSettingsObject> window_settings, NonnullOwnPtr<JS::ExecutionContext> execution_context)
        : EnvironmentSettingsObject(move(execution_context))
        , m_window_settings(window_settings)
    {
        // The id names this object among the event loop's settings objects, so it must not be the page's.
        id = HTML::EnvironmentId::generate();
        creation_url = window_settings->creation_url;
        top_level_creation_url = window_settings->top_level_creation_url;
        top_level_origin = window_settings->top_level_origin;
        target_browsing_context = window_settings->target_browsing_context;
        execution_ready = window_settings->execution_ready;
    }

    virtual GC::Ptr<DOM::Document> responsible_document() override { return m_window_settings->responsible_document(); }
    virtual URL::URL api_base_url() const override { return m_window_settings->api_base_url(); }
    virtual URL::Origin origin() const override { return m_window_settings->origin(); }
    virtual bool has_cross_site_ancestor() const override { return m_window_settings->has_cross_site_ancestor(); }
    virtual NonnullRefPtr<HTML::PolicyContainer> policy_container() const override { return m_window_settings->policy_container(); }
    virtual HTML::CanUseCrossOriginIsolatedAPIs cross_origin_isolated_capability() const override { return m_window_settings->cross_origin_isolated_capability(); }
    virtual Optional<u64> agent_cluster_id() const override { return m_window_settings->agent_cluster_id(); }
    virtual HTML::CrossOriginIsolationMode agent_cluster_cross_origin_isolation_mode() const override { return m_window_settings->agent_cluster_cross_origin_isolation_mode(); }
    virtual double time_origin() const override { return m_window_settings->time_origin(); }

    virtual void visit_edges(Cell::Visitor& visitor) override
    {
        Base::visit_edges(visitor);
        visitor.visit(m_window_settings);
    }

private:
    GC::Ref<HTML::EnvironmentSettingsObject> m_window_settings;
};

GC_DEFINE_ALLOCATOR(ExtensionWorldSettings);

}

GC::Ref<ExtensionWorld> ExtensionWorld::create(HTML::Window& window, String extension_id)
{
    auto& vm = Bindings::main_thread_vm();
    auto browsing_context = window.associated_document().browsing_context();
    VERIFY(browsing_context);
    auto& window_settings = window.relevant_settings_object();
    auto& window_realm = window.principal_realm();

    auto wrapper_world = vm.heap().allocate<Bindings::WrapperWorld>(Bindings::WrapperWorld::Type::Extension);

    // The same steps as for the realm of a window (create_window_realm()), except that the global object is another
    // wrapper of the document's existing Window, and that the global this binding is the WindowProxy this world has.
    auto execution_context = Bindings::create_a_new_javascript_realm(
        vm,
        window_settings.agent_cluster_cross_origin_isolation_mode(),
        [&](JS::Realm& realm) -> GC::Ref<JS::Object> {
            return Bindings::create_global_object_wrapper(realm, GC::Ref { window });
        },
        [&](JS::Realm& realm) -> GC::Ref<JS::Object> {
            return browsing_context->window_proxy_for(*wrapper_world, realm)->object();
        });

    auto& realm = *execution_context->realm();
    auto intrinsics = realm.create<Bindings::Intrinsics>(realm);
    auto host_defined = realm.heap().allocate<ExtensionHostDefined>(intrinsics, *wrapper_world, window_realm);
    realm.set_host_defined(host_defined);
    Bindings::cache_global_object_wrapper(realm);
    MUST(Bindings::initialize_window_web_interfaces(window, realm));

    auto settings = vm.heap().allocate<ExtensionWorldSettings>(window_settings, move(execution_context));
    auto world = vm.heap().allocate<ExtensionWorld>(window, move(extension_id), *wrapper_world, *settings);
    host_defined->world = world;
    return world;
}

ExtensionWorld::ExtensionWorld(HTML::Window& window, String extension_id, Bindings::WrapperWorld& wrapper_world, HTML::EnvironmentSettingsObject& settings)
    : m_extension_id(move(extension_id))
    , m_window(window)
    , m_wrapper_world(wrapper_world)
    , m_settings(settings)
{
}

ExtensionWorld::~ExtensionWorld() = default;

void ExtensionWorld::visit_edges(Cell::Visitor& visitor)
{
    Base::visit_edges(visitor);
    visitor.visit(m_window);
    visitor.visit(m_wrapper_world);
    visitor.visit(m_settings);
}

bool ExtensionWorld::is_extension_realm(JS::Realm const& realm)
{
    return realm.host_defined() && Bindings::host_defined_wrapper_world(realm).type() == Bindings::WrapperWorld::Type::Extension;
}

GC::Ptr<ExtensionWorld> ExtensionWorld::from_realm(JS::Realm const& realm)
{
    if (!is_extension_realm(realm))
        return {};
    return static_cast<ExtensionHostDefined const&>(Bindings::host_defined_of(realm)).world;
}

JS::Realm& ExtensionWorld::realm() const
{
    return m_settings->realm();
}

JS::Object& ExtensionWorld::window_proxy_object() const
{
    return realm().global_environment().global_this_value();
}

void ExtensionWorld::detach()
{
    m_wrapper_world->detach();
}

bool ExtensionWorld::is_detached() const
{
    return m_wrapper_world->is_detached();
}


// The helpers below are what Firefox gives content scripts for crossing into the page. Firefox separates the two sides
// with Xray wrappers and compartments; here they are two realms of one process, and a value that crosses is the object
// itself, so the helpers copy (cloneInto), wrap (exportFunction) or unwrap (wrappedJSObject) explicitly.
//
// LIMITATION: nothing stops a content script from reading a page object directly, so there are no Xray guarantees: a page
// object's getters and prototypes are the page's. What the page can reach is exactly what the script gave it.

namespace {

bool is_own_window(ExtensionWorld& world, JS::Object& object)
{
    return &object == &world.realm().global_object() || &object == &world.window_proxy_object();
}

// `targetScope` as the page's side of the border: the content script's own `window` stands for the page's window.
JS::ThrowCompletionOr<GC::Ref<JS::Object>> resolve_target_scope(JS::VM& vm, ExtensionWorld& world, JS::Value target_scope)
{
    if (!target_scope.is_object())
        return vm.throw_completion<JS::TypeError>("Expected an object as the target scope"_utf16);
    auto& object = target_scope.as_object();
    if (is_own_window(world, object))
        return GC::Ref<JS::Object> { world.window().window()->object() };
    return GC::Ref<JS::Object> { object };
}

JS::Realm& realm_of_scope(JS::Object& object)
{
    return object.shape().realm();
}

JS::ThrowCompletionOr<bool> option_flag(JS::Value options, StringView name)
{
    if (!options.is_object())
        return false;
    return TRY(options.as_object().get(Utf16FlyString::from_utf8(name))).to_boolean();
}

JS::ThrowCompletionOr<GC::Ref<JS::Object>> export_function(JS::VM& vm, JS::Realm& target_realm, JS::FunctionObject& function, Utf16FlyString const& name)
{
    auto length = TRY(function.get(vm.names.length));
    auto exported = JS::NativeFunction::create(
        target_realm,
        GC::create_function<JS::ThrowCompletionOr<JS::Value>(JS::VM&)>(
            target_realm.heap(),
            GC::captures(GC::Ref { function }),
            [](GC::Ref<JS::FunctionObject>& function, JS::VM& vm) -> JS::ThrowCompletionOr<JS::Value> {
                GC::RootVector<JS::Value> arguments;
                for (size_t i = 0; i < vm.argument_count(); ++i)
                    arguments.append(vm.argument(i));
                return JS::call(vm, *function, vm.this_value(), arguments.span());
            }),
        length.is_number() ? static_cast<i32>(length.as_double()) : 0, name,
        // The function belongs to the realm it is exported into: its prototype is that realm's Function.prototype.
        &target_realm);
    return GC::Ref<JS::Object> { exported };
}

JS::ThrowCompletionOr<JS::Value> clone_into(JS::VM& vm, ExtensionWorld& world, JS::Realm& target, JS::Value value, bool clone_functions, HashMap<JS::Object*, JS::Object*>& seen)
{
    if (!value.is_object())
        return value;
    auto& object = value.as_object();
    if (auto it = seen.find(&object); it != seen.end())
        return JS::Value { it->value };

    if (object.is_function()) {
        if (!clone_functions)
            return vm.throw_completion<JS::TypeError>("Permission denied to pass a function; use the cloneFunctions option"_utf16);
        auto exported = TRY(export_function(vm, target, as<JS::FunctionObject>(object), Utf16FlyString::from_utf8(""sv)));
        seen.set(&object, exported.ptr());
        return JS::Value { exported };
    }

    auto& own_realm = world.realm();
    if (TRY(value.is_array(vm))) {
        auto result = MUST(JS::Array::create(target, 0));
        seen.set(&object, result.ptr());
        auto length = TRY(JS::length_of_array_like(vm, object));
        for (size_t i = 0; i < length; ++i) {
            auto element = TRY(object.get(i));
            TRY(result->create_data_property_or_throw(i, TRY(clone_into(vm, world, target, element, clone_functions, seen))));
        }
        return JS::Value { result };
    }

    auto* prototype = object.shape().prototype();
    if (!prototype || prototype == own_realm.intrinsics().object_prototype().ptr()) {
        auto result = JS::Object::create(target, target.intrinsics().object_prototype());
        seen.set(&object, result.ptr());
        auto keys = TRY(object.internal_own_property_keys());
        for (auto const& key : keys) {
            auto property_key = TRY(JS::PropertyKey::from_value(vm, key));
            if (property_key.is_symbol())
                continue;
            auto descriptor = TRY(object.internal_get_own_property(property_key));
            if (!descriptor.has_value() || !descriptor->enumerable.value_or(false))
                continue;
            auto element = TRY(object.get(property_key));
            TRY(result->create_data_property_or_throw(property_key, TRY(clone_into(vm, world, target, element, clone_functions, seen))));
        }
        return JS::Value { result };
    }

    // Dates, maps, typed arrays, errors, blobs and the other types the structured clone algorithm knows. It refuses what
    // it does not know (DOM nodes, functions in them) with a DataCloneError.
    auto& current = *vm.current_realm();
    auto record = TRY(WebIDL::throw_dom_exception_if_needed(vm, current, [&] { return HTML::structured_serialize(vm, value); }));
    return TRY(WebIDL::throw_dom_exception_if_needed(vm, current, [&] { return HTML::structured_deserialize(vm, record, target); }));
}

JS::ThrowCompletionOr<JS::Value> to_page(ExtensionWorld& world, JS::Value value)
{
    if (!value.is_object())
        return value;
    auto& object = value.as_object();
    auto& window = world.window();
    if (is_own_window(world, object))
        return JS::Value { &window.window()->object() };
    if (auto* other_window = HTML::WindowProxy::from_object(object)) {
        if (auto target = other_window->window())
            return JS::Value { &target->window()->object() };
        return value;
    }
    if (auto* wrappable = Bindings::wrappable_impl_from(&object)) {
        if (auto* as_window = Bindings::impl_from<HTML::Window>(&object))
            return JS::Value { &as_window->window()->object() };
        auto& main_realm = window.principal_realm();
        return JS::Value { Bindings::wrap(Bindings::host_defined_wrapper_world(main_realm), main_realm, GC::Ref { *wrappable }) };
    }
    // Plain objects of this world have no page-side counterpart.
    return value;
}

}

void ExtensionWorld::install_firefox_helpers()
{
    auto& realm = this->realm();
    auto& vm = realm.vm();
    auto& global = realm.global_object();
    auto world = GC::Ref { *this };
    auto const hidden = JS::Attribute::Configurable | JS::Attribute::Writable;

    global.define_native_function(
        realm, "cloneInto"_utf16_fly_string,
        GC::create_function<JS::ThrowCompletionOr<JS::Value>(JS::VM&)>(
            realm.heap(),
            GC::captures(world),
            [](GC::Ref<ExtensionWorld>& world, JS::VM& vm) -> JS::ThrowCompletionOr<JS::Value> {
                auto scope = TRY(resolve_target_scope(vm, *world, vm.argument(1)));
                auto clone_functions = TRY(option_flag(vm.argument(2), "cloneFunctions"sv));
                HashMap<JS::Object*, JS::Object*> seen;
                return clone_into(vm, *world, realm_of_scope(*scope), vm.argument(0), clone_functions, seen);
            }),
        3, hidden);

    global.define_native_function(
        realm, "exportFunction"_utf16_fly_string,
        GC::create_function<JS::ThrowCompletionOr<JS::Value>(JS::VM&)>(
            realm.heap(),
            GC::captures(world),
            [](GC::Ref<ExtensionWorld>& world, JS::VM& vm) -> JS::ThrowCompletionOr<JS::Value> {
                auto function = vm.argument(0);
                if (!function.is_function())
                    return vm.throw_completion<JS::TypeError>("Expected a function to export"_utf16);
                auto scope = TRY(resolve_target_scope(vm, *world, vm.argument(1)));
                auto options = vm.argument(2);
                Utf16FlyString name;
                if (options.is_object()) {
                    auto define_as = TRY(options.as_object().get("defineAs"_utf16_fly_string));
                    if (!define_as.is_undefined())
                        name = Utf16FlyString { TRY(define_as.to_utf16_string(vm)) };
                }
                if (name.is_empty()) {
                    auto function_name = TRY(function.as_object().get(vm.names.name));
                    if (function_name.is_string())
                        name = Utf16FlyString { function_name.as_string().utf16_string() };
                }
                auto exported = TRY(export_function(vm, realm_of_scope(*scope), function.as_function(), name));
                if (!name.is_empty() && options.is_object() && !TRY(options.as_object().get("defineAs"_utf16_fly_string)).is_undefined())
                    TRY(scope->create_data_property_or_throw(name, JS::Value { exported }));
                return JS::Value { exported };
            }),
        3, hidden);

    global.define_native_function(
        realm, "createObjectIn"_utf16_fly_string,
        GC::create_function<JS::ThrowCompletionOr<JS::Value>(JS::VM&)>(
            realm.heap(),
            GC::captures(world),
            [](GC::Ref<ExtensionWorld>& world, JS::VM& vm) -> JS::ThrowCompletionOr<JS::Value> {
                auto scope = TRY(resolve_target_scope(vm, *world, vm.argument(0)));
                auto& target = realm_of_scope(*scope);
                auto result = JS::Object::create(target, target.intrinsics().object_prototype());
                auto options = vm.argument(1);
                if (options.is_object()) {
                    auto define_as = TRY(options.as_object().get("defineAs"_utf16_fly_string));
                    if (!define_as.is_undefined())
                        TRY(scope->create_data_property_or_throw(Utf16FlyString { TRY(define_as.to_utf16_string(vm)) }, JS::Value { result }));
                }
                return JS::Value { result };
            }),
        2, hidden);

    // `x.wrappedJSObject` for the objects that have a counterpart in the page: windows, documents, elements, anything
    // that is an EventTarget.
    auto event_target = global.get("EventTarget"_utf16_fly_string);
    if (event_target.is_error() || !event_target.value().is_object())
        return;
    auto prototype = event_target.release_value().as_object().get(vm.names.prototype);
    if (prototype.is_error() || !prototype.value().is_object())
        return;
    prototype.release_value().as_object().define_native_accessor(
        realm, "wrappedJSObject"_utf16_fly_string,
        GC::create_function<JS::ThrowCompletionOr<JS::Value>(JS::VM&)>(
            realm.heap(),
            GC::captures(world),
            [](GC::Ref<ExtensionWorld>& world, JS::VM& vm) -> JS::ThrowCompletionOr<JS::Value> { return to_page(*world, vm.this_value()); }),
        nullptr, JS::Attribute::Configurable);
}

}
