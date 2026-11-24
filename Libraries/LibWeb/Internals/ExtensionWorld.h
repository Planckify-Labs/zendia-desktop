/*
 * Copyright (c) 2026, cgx-webext contributors
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

#pragma once

#include <AK/String.h>
#include <LibGC/Ptr.h>
#include <LibJS/Heap/Cell.h>
#include <LibWeb/Export.h>
#include <LibWeb/Forward.h>

namespace Web::Internals {

// AD-HOC (cgx-webext): The isolated world of one browser extension in one document, which is where the extension's
//                      content scripts run (Chrome's model: one world per document and extension).
//
// It has the document's DOM and its own JavaScript: a realm of its own whose global object is another wrapper of the
// document's Window, a WindowProxy of its own, a WrapperWorld of type Extension (so DOM objects have their own wrappers
// with their own prototypes and expandos), and an environment settings object of its own, which is the page's in every
// respect but its realm and its module map. The page's script cannot reach any of it, and a built-in that the page
// replaced is not replaced here.
class WEB_API ExtensionWorld final : public JS::Cell {
    GC_CELL(ExtensionWorld, JS::Cell);
    GC_DECLARE_ALLOCATOR(ExtensionWorld);

public:
    // `window` must belong to a document that has a browsing context.
    static GC::Ref<ExtensionWorld> create(HTML::Window& window, String extension_id);
    virtual ~ExtensionWorld() override;

    // Whether the realm belongs to an extension world, which the page's CSP and Trusted Types policy do not govern.
    static bool is_extension_realm(JS::Realm const&);
    // The extension world a realm belongs to, if it is one's.
    static GC::Ptr<ExtensionWorld> from_realm(JS::Realm const&);

    String const& extension_id() const { return m_extension_id; }
    HTML::Window& window() const { return m_window; }
    JS::Realm& realm() const;
    HTML::EnvironmentSettingsObject& settings() const { return m_settings; }
    Bindings::WrapperWorld& wrapper_world() const { return m_wrapper_world; }
    // The WindowProxy that is this world's `window`.
    JS::Object& window_proxy_object() const;

    // Firefox extensions' content scripts have `cloneInto`, `exportFunction`, `createObjectIn` and `wrappedJSObject`, which
    // are how they pass objects and functions to the page. Defines them in this world.
    void install_firefox_helpers();

    // Releases what the world keeps alive in the document's objects. After this, wrappers made in the world are not
    // cached or preserved. The document calls it when it is destroyed.
    void detach();
    bool is_detached() const;

    virtual void visit_edges(Cell::Visitor&) override;

private:
    ExtensionWorld(HTML::Window&, String extension_id, Bindings::WrapperWorld&, HTML::EnvironmentSettingsObject&);

    String m_extension_id;
    GC::Ref<HTML::Window> m_window;
    GC::Ref<Bindings::WrapperWorld> m_wrapper_world;
    GC::Ref<HTML::EnvironmentSettingsObject> m_settings;
};

}
