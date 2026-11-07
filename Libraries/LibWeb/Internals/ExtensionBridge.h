/*
 * Copyright (c) 2026, cgx-webext contributors
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

#pragma once

#include <AK/HashMap.h>
#include <AK/String.h>
#include <AK/Utf16String.h>
#include <LibGC/Ptr.h>
#include <LibWeb/Export.h>
#include <LibWeb/Forward.h>
#include <LibWebCommon/HTML/CrossProcessId.h>
#include <LibWeb/Internals/InternalsBase.h>
#include <LibWeb/WebIDL/CallbackType.h>
#include <LibWeb/WebIDL/Promise.h>

namespace Web::Internals {

// AD-HOC (cgx-webext): Connects extension JavaScript to the extension core in the browser process.
//
// An extension's own pages (background page, popup, options) get one as `__cgxBridge`, which Shim/boot.js consumes.
// Content scripts get one per extension that is handed to their program as an argument and is never put on the page's
// global object. The browser process decides what each bridge may do; nothing the renderer says is trusted there.
class WEB_API ExtensionBridge final : public InternalsBase {
    WEB_WRAPPABLE(ExtensionBridge, InternalsBase);
    GC_DECLARE_ALLOCATOR(ExtensionBridge);

public:
    // `extension_id` is empty for an extension's own page.
    static GC::Ref<ExtensionBridge> create(HTML::Window&, String extension_id);
    virtual ~ExtensionBridge() override;

    static bool is_extension_origin(URL::Origin const&);
    // Defines `__cgxBridge` for an extension's own page.
    static void install_for_extension_page(JS::Realm&, HTML::Window&);
    // Runs the content scripts of every extension that match the document. Called when it may first run script, before
    // its parser starts, which is when `document_start` scripts run.
    static void inject_content_scripts(DOM::Document&);
    // Tells the content scripts of the document that a later stage ("document_end", "document_idle") was reached.
    static void run_content_script_stage(DOM::Document&, StringView stage);

    Utf16String context_init();
    GC::Ref<WebIDL::Promise> call(Utf16String const& name, Utf16String const& args_json);
    Utf16String call_sync(Utf16String const& name, Utf16String const& args_json);
    void set_message_handler(WebIDL::CallbackType& handler);

    // From the browser process.
    void complete_call(u64 request_id, bool ok, String const& json);
    void deliver(String const& json);

    String const& extension_id() const { return m_extension_id; }

private:
    ExtensionBridge(HTML::Window&, String extension_id);
    virtual void visit_edges(GC::Cell::Visitor&) override;

    HTML::CrossProcessId document_id() const;

    String m_extension_id;
    GC::Ptr<WebIDL::CallbackType> m_message_handler;
    HashMap<u64, GC::Ref<WebIDL::Promise>> m_pending_calls;
    u64 m_next_request_id { 1 };
};

}
