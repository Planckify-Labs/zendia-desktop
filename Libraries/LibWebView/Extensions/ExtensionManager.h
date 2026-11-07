/*
 * Copyright (c) 2026, cgx-webext contributors
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

#pragma once

#include <AK/ByteBuffer.h>
#include <AK/ByteString.h>
#include <AK/Function.h>
#include <AK/NonnullOwnPtr.h>
#include <AK/Optional.h>
#include <AK/OwnPtr.h>
#include <AK/String.h>
#include <AK/StringView.h>
#include <AK/Types.h>
#include <AK/Variant.h>
#include <LibWebView/Forward.h>

struct WebExtHandle;

namespace WebView {

struct ExtensionCallbackContext;

// Owns the WebExtensions core (the cgx-webext Rust library) and adapts its C ABI to Ladybird types.
//
// The core decides everything that does not need the browser: manifest parsing and validation, permissions, the
// chrome.*/browser.* API surface, storage, i18n, alarms, which content scripts match a URL, and which extension files a web
// page may load. Whatever needs the browser goes through Delegate.
//
// Single-threaded: use on the thread that created it (the Browser process main thread).
class WEBVIEW_API ExtensionManager {
    AK_ALLOC_WITH_KMALLOC;
    AK_MAKE_NONCOPYABLE(ExtensionManager);
    AK_MAKE_NONMOVABLE(ExtensionManager);

public:
    class Delegate {
    public:
        virtual ~Delegate() = default;

        // Persistence, one blob per (extension, key); `key` is "local", "sync" or "permissions". Values are JSON.
        virtual Optional<ByteString> read_extension_data(StringView extension_id, StringView key) = 0;
        virtual void write_extension_data(StringView extension_id, StringView key, StringView json) = 0;

        // Deliver an event to every context of the extension (background, popup, content scripts). `args_json` is a JSON
        // array of the listener arguments. Hand it to `api.dispatchEvent(event, args)` of Shim/webext-shim.js.
        virtual void dispatch_event(StringView extension_id, StringView event, StringView args_json) = 0;

        // Carries an envelope produced by the core's message bus to one context, which hands it to `handleHostMessage` of its
        // Shim/webext-shim.js. Envelopes to one context must be delivered in order. Delivering to a context that is gone is
        // not an error.
        virtual void deliver_to_context(u64 context_id, StringView envelope_json) = 0;

        // The full `tabs.Tab` JSON object for `MessageSender.tab`; without it the sender only carries the tab id.
        virtual Optional<ByteString> describe_tab(i64 tab_id) { (void)tab_id; return {}; }

        // Implement an API the core delegates (tabs.*, windows.*, action.*, cookies.*, ...). `call_json` is an ApiCall.
        // Call `reply` exactly once with the result JSON, or an error message. Must not outlive the ExtensionManager.
        virtual void call_browser(StringView call_json, Function<void(ErrorOr<ByteString>)> reply) = 0;

        // Ask the user to grant optional permissions; `reply(true)` only if they agreed.
        virtual void request_permissions(StringView extension_id, StringView request_json, Function<void(bool)> reply) = 0;

        // The earliest pending alarm is due at `at_ms` (Unix milliseconds): call ExtensionManager::tick() then.
        virtual void schedule_alarm_check(i64 at_ms) = 0;

        // BCP 47 UI language, e.g. "en-US".
        virtual String ui_locale() { return "en-US"_string; }
        // Unix milliseconds; overridable for tests.
        virtual i64 now_ms();
    };

    static NonnullOwnPtr<ExtensionManager> create(Delegate&);
    ~ExtensionManager();

    struct InstalledExtension {
        String id;
        String display_name;
        String version;
        // Where the extension is served from, e.g. "chrome-extension://<id>/" (chosen by the core when "auto" was passed).
        String base_url;
        // The raw result JSON of the core, including warnings.
        ByteString result_json;
    };

    // Why a package was refused: every problem found, joined with "; ".
    struct InstallFailure {
        String message;
    };

    // Validates and installs a .crx/.xpi/.zip. `id_override` may be empty if the package carries an ID. `base_url` is the
    // origin the extension is served from and must end in '/', e.g. "chrome-extension://<id>/".
    // A package that fails validation is an InstallFailure; the ErrorOr is for allocation failures only.
    ErrorOr<Variant<InstalledExtension, InstallFailure>> install_package(ReadonlyBytes package, StringView id_override, StringView base_url);
    // Installs an unpacked extension directory ("Load unpacked"). Same result as install_package.
    ErrorOr<Variant<InstalledExtension, InstallFailure>> install_directory(StringView path, StringView id_override, StringView base_url);
    bool uninstall(StringView extension_id);

    // The answer to a request for an extension URL; see webext_serve. `initiator` is the serialized origin of the requester,
    // or empty for a navigation the browser started itself.
    struct ServedResource {
        u16 status { 0 };
        ByteString mime_type;
        Optional<ByteString> content_security_policy;
        ByteString extension_id;
        ByteBuffer body;
    };
    Optional<ServedResource> serve(StringView url, StringView initiator);
    // The content scripts of all extensions that match a document, with sources inlined (see webext_content_script_bundles).
    ByteString content_script_bundles(StringView url, bool is_top_frame, StringView fallback_url = {});
    // Per-extension content-script programs for a document (see webext_content_script_programs).
    ByteString content_script_programs(StringView url, bool is_top_frame, StringView fallback_url = {});
    // Where to load the extension's background context from, if it has one.
    Optional<ByteString> background_url(StringView extension_id);
    // Whether a document at `url` is the extension's background context rather than a popup, options page or tab.
    bool is_background_url(StringView extension_id, StringView url);
    // The id of the extension that owns `url`, if it is an installed extension's URL.
    Optional<ByteString> extension_for_url(StringView url);

    // Runs an API call (ApiCall JSON). `on_reply(ok, json)` is called exactly once, possibly before this returns.
    void dispatch(StringView call_json, Function<void(bool ok, StringView json)> on_reply);

    // What to host the extension with, as JSON: background context to start, CSP for extension pages, toolbar action, options
    // page, icons, permissions. See Dispatcher::info_for in the core.
    Optional<ByteString> extension_info(StringView extension_id);

    // A place extension code runs. Register every context before it makes an API call and unregister it when it goes away
    // (tab closed, frame navigated, background shut down): that is what lets the core route messages and close ports.
    enum class ContextKind {
        Background,
        ExtensionPage,
        ContentScript,
        Other,
    };
    struct ContextDescription {
        u64 id { 0 };
        StringView extension_id;
        ContextKind kind { ContextKind::Other };
        Optional<i64> tab_id;
        Optional<i64> frame_id;
        StringView url;
    };
    ErrorOr<bool> context_created(ContextDescription const&);
    void context_destroyed(u64 context_id);

    // Like init_data() for one registered context; for content scripts the API is trimmed to what they may use.
    Optional<ByteString> init_data_for_context(u64 context_id);

    // JSON the page-side shim needs to build chrome.* / browser.* (see Shim/webext-shim.js).
    Optional<ByteString> init_data(StringView extension_id);

    // [{index, js, css, runAt, world, allFrames}] describing the content scripts to run in a document.
    Optional<ByteString> content_scripts_for(StringView extension_id, StringView url, bool is_top_frame, StringView fallback_url = {});

    enum class Initiator : u32 {
        SameExtension = 0,
        WebPage = 1,
        OtherExtension = 2,
    };
    // Whether `initiator` may load `path` from the extension (web_accessible_resources).
    bool is_web_accessible(StringView extension_id, StringView path, Initiator, StringView initiator_value);

    // The bytes of a file in the extension package, if it exists. Check is_web_accessible first for web page requests.
    Optional<ByteBuffer> read_file(StringView extension_id, StringView path);

    // Fires due alarms. Call when the timer requested through Delegate::schedule_alarm_check elapses.
    void tick();

    void notify_installed(StringView extension_id, StringView reason, StringView previous_version = {});
    void notify_startup(StringView extension_id);

    // Extension stores (the Chrome Web Store and addons.mozilla.org); see docs/STORES.md in cgx-webext.
    // content_script_programs() also returns the store program, under the bridge key "__store__", for store pages.
    static constexpr StringView store_bridge_key = "__store__"sv;
    // "chrome" or "firefox" for a store page URL.
    static Optional<ByteString> store_for_url(StringView url);
    // chrome.management.ExtensionInfo JSON of every installed extension (an array) or, given an id, of one.
    Optional<ByteString> management_info(StringView extension_id = {});
    // addons.mozilla.org Addon JSON of an installed extension.
    Optional<ByteString> amo_addon(StringView extension_id);
    // {"ok","name","version","warnings"} for the install prompt, from a manifest a store sent, or for an installed extension
    // when `manifest_json` is empty.
    ByteString preview_store_manifest(StringView manifest_json, StringView localized_name, StringView extension_id = {});
    // Checks a downloaded store package against what the store asked for (empty = no expectation):
    // {"ok":true,"id","name","version","warnings"} or {"ok":false,"error"}.
    static ByteString check_store_package(ReadonlyBytes package, StringView expected_id, StringView expected_hash, StringView approved_manifest);
    // The Chrome Web Store URL that redirects to an extension's .crx.
    static Optional<ByteString> chrome_web_store_download_url(StringView extension_id, StringView chrome_version);

private:
    explicit ExtensionManager(Delegate&);

    // What the C callbacks receive as user data. Declared before the handle so it outlives it.
    OwnPtr<ExtensionCallbackContext> m_context;
    WebExtHandle* m_handle { nullptr };
};

}
