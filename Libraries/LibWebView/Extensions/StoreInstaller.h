/*
 * Copyright (c) 2026, cgx-webext contributors
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

#pragma once

#if defined(ENABLE_WEBEXT)

#    include <AK/ByteBuffer.h>
#    include <AK/ByteString.h>
#    include <AK/Function.h>
#    include <AK/HashMap.h>
#    include <AK/HashTable.h>
#    include <AK/JsonArray.h>
#    include <AK/RefPtr.h>
#    include <AK/String.h>
#    include <AK/Vector.h>
#    include <AK/WeakPtr.h>
#    include <AK/Weakable.h>
#    include <LibRequests/Forward.h>
#    include <LibURL/URL.h>
#    include <LibWebView/Forward.h>

namespace WebView {

class ExtensionManager;

// AD-HOC (cgx-webext): Makes the install buttons of the extension stores work. The Chrome Web Store ("Add to Chrome") and
// addons.mozilla.org ("Add to Firefox") call private browser APIs from their pages; the store program the core injects
// there (see webext::store in cgx-webext) forwards those calls here through the store bridge. This downloads the package
// from the store, checks it is what the store offered, asks the user, installs it and keeps it in the profile so that it
// is loaded again at the next start.
//
// Only calls from documents the browser knows to be on a store's origin reach this (see WebContentPage).
class StoreInstaller : public Weakable<StoreInstaller> {
    AK_ALLOC_WITH_KMALLOC;
    AK_MAKE_NONCOPYABLE(StoreInstaller);
    AK_MAKE_NONMOVABLE(StoreInstaller);

public:
    // Packages and their index (index.json) are kept in `data_directory`.
    StoreInstaller(ExtensionManager&, ByteString data_directory);
    ~StoreInstaller();

    // Installs the packages kept from earlier store installs. Returns their extension ids.
    Vector<ByteString> load_installed_packages();

    struct Caller {
        u64 context_id { 0 };
        ByteString store; // "chrome" or "firefox"
    };
    using Reply = Function<void(bool ok, String json)>;
    // A call from a store page: `name` is e.g. "cws.beginInstall", `args_json` a JSON array.
    void handle_call(Caller const&, String const& name, String const& args_json, Reply);

    // Removes an extension, whoever asked, and tells the store pages.
    bool uninstall(ByteString const& extension_id);

    // A store install finished: start the extension (its background page). `is_update` if it replaced an installed version.
    Function<void(ByteString const& extension_id, bool is_update)> on_installed;
    // An extension was removed: stop it.
    Function<void(ByteString const& extension_id)> on_uninstalled;

private:
    struct Download;
    using DownloadCallback = Function<void(ErrorOr<ByteBuffer>)>;
    void download(URL::URL, DownloadCallback, u32 redirects = 0);

    void handle_chrome_call(Caller const&, String const& name, JsonArray const& args, Reply);
    void handle_firefox_call(Caller const&, String const& name, JsonArray const& args, Reply);

    void chrome_begin_install(JsonArray const& args, Reply);
    void chrome_complete_install(ByteString const& extension_id, Reply);
    void firefox_install(Caller const&, i64 install_id, String const& url, String const& hash, Reply);

    // Installs a checked package, keeps it, and announces it. Returns the installed id.
    ErrorOr<ByteString> install_and_keep(ReadonlyBytes package, StringView store);
    void announce_installed(ByteString const& extension_id);
    void announce_uninstalled(ByteString const& extension_id);

    bool is_installed(StringView extension_id) const;
    ByteString packages_directory() const;
    ByteString index_path() const;
    void write_index() const;

    ExtensionManager& m_manager;
    ByteString m_data_directory;

    struct KeptPackage {
        ByteString extension_id;
        ByteString file_name;
        ByteString store;
    };
    Vector<KeptPackage> m_kept_packages;

    // Chrome Web Store installs the user approved in beginInstallWithManifest3, waiting for completeInstall: id -> manifest.
    HashMap<ByteString, ByteString> m_approved_installs;
    HashTable<ByteString> m_installs_in_progress;

    HashMap<u64, NonnullOwnPtr<Download>> m_downloads;
    u64 m_next_download_id { 1 };
};

}

#endif
