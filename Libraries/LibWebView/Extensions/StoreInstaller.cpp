/*
 * Copyright (c) 2026, cgx-webext contributors
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

#if defined(ENABLE_WEBEXT)

#    include <AK/CharacterTypes.h>
#    include <AK/JsonObject.h>
#    include <AK/JsonValue.h>
#    include <AK/LexicalPath.h>
#    include <AK/ScopeGuard.h>
#    include <AK/StringBuilder.h>
#    include <LibCore/Directory.h>
#    include <LibCore/EventLoop.h>
#    include <LibCore/File.h>
#    include <LibCore/System.h>
#    include <LibCore/Timer.h>
#    include <LibFileSystem/FileSystem.h>
#    include <LibHTTP/HeaderList.h>
#    include <LibRequests/NetworkError.h>
#    include <LibRequests/Request.h>
#    include <LibRequests/RequestClient.h>
#    include <LibURL/Parser.h>
#    include <LibWebCommon/Fetch/Infrastructure/HTTP/Statuses.h>
#    include <LibWebCommon/Loader/UserAgent.h>
#    include <LibWebView/Application.h>
#    include <LibWebView/Extensions/ExtensionManager.h>
#    include <LibWebView/Extensions/StoreInstaller.h>
#    include <LibWebView/Utilities.h>
#    include <LibWebView/WebContentPage.h>

namespace WebView {

// Store packages are a few megabytes; nothing a store serves comes near this.
static constexpr size_t maximum_package_size = 256 * MiB;
static constexpr u32 maximum_redirects = 10;

struct StoreInstaller::Download {
    AK_ALLOC_WITH_KMALLOC;
    RefPtr<Requests::Request> request;
    RefPtr<Core::Timer> timeout;
};

static constexpr auto download_timeout = AK::Duration::from_seconds(120);

static String json_string(StringView value)
{
    return JsonValue { MUST(String::from_utf8(value)) }.serialized();
}

static StringView store_name(StringView store)
{
    return store == "chrome"sv ? "the Chrome Web Store"sv : "addons.mozilla.org"sv;
}

static ByteString store_page_key(StringView store)
{
    return ByteString::formatted("{}{}", ExtensionManager::store_bridge_key, store);
}

// File names come from extension ids, which come from packages: keep them to a safe alphabet.
static ByteString file_name_for(StringView extension_id, StringView extension)
{
    StringBuilder builder;
    builder.append('_');
    for (auto character : extension_id)
        builder.append(is_ascii_alphanumeric(character) || character == '.' || character == '-' || character == '_' || character == '@' ? character : '_');
    builder.appendff(".{}", extension);
    return builder.to_byte_string();
}

// The Chrome version Ladybird reports in its User-Agent string, which the Chrome Web Store uses to pick a package.
static StringView reported_chrome_version()
{
    auto user_agent = Web::default_user_agent;
    auto start = user_agent.find("Chrome/"sv);
    if (!start.has_value())
        return "153.0.0.0"sv;
    auto version = user_agent.substring_view(*start + 7);
    if (auto end = version.find(' '); end.has_value())
        version = version.substring_view(0, *end);
    return version;
}

static Optional<JsonObject> parse_object(StringView json)
{
    auto value = JsonValue::from_string(json);
    if (value.is_error() || !value.value().is_object())
        return {};
    return value.value().as_object();
}

static Vector<String> warnings_of(JsonObject const& preview)
{
    Vector<String> warnings;
    if (auto list = preview.get_array("warnings"sv); list.has_value()) {
        list->for_each([&](JsonValue const& warning) {
            if (warning.is_string())
                warnings.append(warning.as_string());
        });
    }
    return warnings;
}

static String string_arg(JsonArray const& args, size_t index)
{
    if (index >= args.size() || !args.at(index).is_string())
        return {};
    return args.at(index).as_string();
}

bool StoreInstaller::is_installed(StringView extension_id) const
{
    return !extension_id.is_empty() && m_manager.management_info(extension_id).has_value();
}

StoreInstaller::StoreInstaller(ExtensionManager& manager, ByteString data_directory)
    : m_manager(manager)
    , m_data_directory(move(data_directory))
{
}

StoreInstaller::~StoreInstaller() = default;

ByteString StoreInstaller::packages_directory() const
{
    return m_data_directory;
}

ByteString StoreInstaller::index_path() const
{
    return LexicalPath::join(m_data_directory, "index.json"sv).string();
}

void StoreInstaller::write_index() const
{
    JsonArray index;
    for (auto const& kept : m_kept_packages) {
        JsonObject entry;
        entry.set("id"sv, MUST(String::from_byte_string(kept.extension_id)));
        entry.set("file"sv, MUST(String::from_byte_string(kept.file_name)));
        entry.set("store"sv, MUST(String::from_byte_string(kept.store)));
        index.must_append(move(entry));
    }
    JsonObject root;
    root.set("extensions"sv, move(index));
    if (auto result = Core::Directory::create(m_data_directory, Core::Directory::CreateDirectories::Yes); result.is_error())
        warnln("Extensions: cannot create {}: {}", m_data_directory, result.error());
    if (auto result = write_json_file(index_path(), root); result.is_error())
        warnln("Extensions: cannot write {}: {}", index_path(), result.error());
}

Vector<ByteString> StoreInstaller::load_installed_packages()
{
    Vector<ByteString> loaded;
    if (!FileSystem::exists(index_path()))
        return loaded;

    auto index = read_json_file(index_path());
    auto entries = index.is_error() ? Optional<JsonArray const&> {} : index.value().get_array("extensions"sv);
    if (!entries.has_value()) {
        warnln("Extensions: cannot read {}", index_path());
        return loaded;
    }

    entries->for_each([&](JsonValue const& value) {
        if (!value.is_object())
            return;
        auto id = value.as_object().get_string("id"sv);
        auto file = value.as_object().get_string("file"sv);
        auto store = value.as_object().get_string("store"sv).value_or("chrome"_string);
        if (!id.has_value() || !file.has_value() || file->contains('/') || file->starts_with('.'))
            return;

        auto path = LexicalPath::join(packages_directory(), *file).string();
        auto file_handle = Core::File::open(path, Core::File::OpenMode::Read);
        auto contents = file_handle.is_error() ? ErrorOr<ByteBuffer> { file_handle.release_error() } : file_handle.value()->read_until_eof();
        if (contents.is_error()) {
            warnln("Extensions: cannot read {}: {}", path, contents.error());
            return;
        }
        auto result = m_manager.install_package(contents.value().bytes(), {}, "auto"sv);
        if (result.is_error() || !result.value().has<ExtensionManager::InstalledExtension>()) {
            warnln("Extensions: the kept package {} no longer installs", path);
            return;
        }
        auto const& installed = result.value().get<ExtensionManager::InstalledExtension>();
        outln("Loaded extension: {} {} at {} (from {})", installed.display_name, installed.version, installed.base_url, store_name(store));
        m_kept_packages.append({ installed.id.to_byte_string(), file->to_byte_string(), store.to_byte_string() });
        loaded.append(installed.id.to_byte_string());
    });
    return loaded;
}

void StoreInstaller::download(URL::URL url, DownloadCallback callback, u32 redirects)
{
    if (url.scheme() != "https"sv) {
        callback(Error::from_string_literal("Store packages are only downloaded over HTTPS"));
        return;
    }
    auto client = Application::request_server_client();
    if (client.is_error()) {
        callback(Error::from_string_literal("Cannot reach the network service"));
        return;
    }

    auto headers = HTTP::HeaderList::create();
    headers->set({ "User-Agent"sv, Web::default_user_agent });
    auto request = client.value()->start_request("GET"sv, url, *headers, {}, HTTP::CacheMode::NoStore, HTTP::Cookie::IncludeCredentials::No);
    if (!request) {
        callback(Error::from_string_literal("Cannot start the download"));
        return;
    }

    auto download_id = m_next_download_id++;
    // Exactly one of the response and the timeout answers.
    struct Pending : RefCounted<Pending> {
        DownloadCallback callback;
    };
    auto pending = adopt_ref(*new Pending);
    pending->callback = move(callback);

    auto entry = make<Download>();
    entry->request = request;
    entry->timeout = Core::Timer::create_single_shot(download_timeout, [this, download_id, pending, url] {
        if (auto entry = m_downloads.get(download_id); entry.has_value())
            (*entry)->request->stop();
        Core::deferred_invoke([weak_this = make_weak_ptr(), download_id] {
            if (weak_this)
                weak_this->m_downloads.remove(download_id);
        });
        dbgln("Extensions: downloading {} timed out", url);
        if (auto callback = move(pending->callback))
            callback(Error::from_string_literal("The download timed out"));
    });
    entry->timeout->start();
    m_downloads.set(download_id, move(entry));

    request->set_buffered_request_finished_callback(
        [this, download_id, url, redirects, pending](u64, Requests::RequestTimingInfo const&, Optional<Requests::NetworkError> const& network_error, NonnullRefPtr<HTTP::HeaderList> response_headers, Optional<u32> response_code, Optional<String>, Requests::CacheState, Core::ImmutableBytes payload) mutable {
            auto callback = move(pending->callback);
            if (!callback)
                return;
            if (auto entry = m_downloads.get(download_id); entry.has_value())
                (*entry)->timeout->stop();
            // The request must not be destroyed from inside its own callback.
            Core::deferred_invoke([weak_this = make_weak_ptr(), download_id] {
                if (weak_this)
                    weak_this->m_downloads.remove(download_id);
            });

            if (network_error.has_value()) {
                dbgln("Extensions: downloading {} failed: {}", url, Requests::network_error_to_string(*network_error));
                callback(Error::from_string_literal("Network error while downloading the extension"));
                return;
            }
            auto status = response_code.value_or(0);
            if (Web::Fetch::Infrastructure::is_redirect_status(status)) {
                auto location = response_headers->get("Location"sv);
                auto next = location.has_value() ? URL::Parser::basic_parse(*location, url) : Optional<URL::URL> {};
                if (!next.has_value() || redirects >= maximum_redirects) {
                    callback(Error::from_string_literal("The store redirected the download badly"));
                    return;
                }
                download(next.release_value(), move(callback), redirects + 1);
                return;
            }
            if (status < 200 || status >= 300) {
                dbgln("Extensions: downloading {} answered HTTP {}", url, status);
                callback(Error::from_string_literal("The store did not deliver the extension"));
                return;
            }
            if (payload.size() > maximum_package_size) {
                callback(Error::from_string_literal("The extension package is too large"));
                return;
            }
            auto bytes = ByteBuffer::copy(payload.bytes());
            if (bytes.is_error()) {
                callback(bytes.release_error());
                return;
            }
            callback(bytes.release_value());
        });
}

ErrorOr<ByteString> StoreInstaller::install_and_keep(ReadonlyBytes package, StringView store)
{
    auto result = TRY(m_manager.install_package(package, {}, "auto"sv));
    if (auto const* failure = result.get_pointer<ExtensionManager::InstallFailure>()) {
        warnln("Extensions: the store package does not install: {}", failure->message);
        return Error::from_string_literal("The extension could not be installed");
    }
    auto const& installed = result.get<ExtensionManager::InstalledExtension>();
    auto id = installed.id.to_byte_string();

    auto file_name = file_name_for(id, store == "chrome"sv ? "crx"sv : "xpi"sv);
    TRY(Core::Directory::create(packages_directory(), Core::Directory::CreateDirectories::Yes));
    auto path = LexicalPath::join(packages_directory(), file_name).string();
    auto temporary_path = ByteString::formatted("{}.part", path);
    {
        auto file = TRY(Core::File::open(temporary_path, Core::File::OpenMode::Write | Core::File::OpenMode::Truncate));
        TRY(file->write_until_depleted(package));
    }
    TRY(FileSystem::move_file(path, temporary_path, FileSystem::PreserveMode::Nothing));

    m_kept_packages.remove_all_matching([&](auto const& kept) { return kept.extension_id == id; });
    m_kept_packages.append({ id, file_name, store });
    write_index();

    outln("Installed extension from {}: {} {} at {}", store_name(store), installed.display_name, installed.version, installed.base_url);
    return id;
}

void StoreInstaller::announce_installed(ByteString const& extension_id)
{
    if (auto info = m_manager.management_info(extension_id); info.has_value()) {
        auto message = ByteString::formatted("{{\"kind\":\"event\",\"name\":\"management.onInstalled\",\"args\":[{}]}}", *info);
        deliver_event_to_extension(store_page_key("chrome"sv), message);
    }
    auto id = json_string(extension_id);
    deliver_event_to_extension(store_page_key("firefox"sv), ByteString::formatted("{{\"kind\":\"event\",\"name\":\"onInstalling\",\"id\":{}}}", id));
    deliver_event_to_extension(store_page_key("firefox"sv), ByteString::formatted("{{\"kind\":\"event\",\"name\":\"onInstalled\",\"id\":{}}}", id));
}

void StoreInstaller::announce_uninstalled(ByteString const& extension_id)
{
    auto id = json_string(extension_id);
    deliver_event_to_extension(store_page_key("chrome"sv), ByteString::formatted("{{\"kind\":\"event\",\"name\":\"management.onUninstalled\",\"args\":[{}]}}", id));
    deliver_event_to_extension(store_page_key("firefox"sv), ByteString::formatted("{{\"kind\":\"event\",\"name\":\"onUninstalling\",\"id\":{}}}", id));
    deliver_event_to_extension(store_page_key("firefox"sv), ByteString::formatted("{{\"kind\":\"event\",\"name\":\"onUninstalled\",\"id\":{}}}", id));
}

bool StoreInstaller::uninstall(ByteString const& extension_id)
{
    if (!is_installed(extension_id))
        return false;

    if (on_uninstalled)
        on_uninstalled(extension_id);
    m_manager.uninstall(extension_id);

    if (auto kept = m_kept_packages.find_if([&](auto const& kept) { return kept.extension_id == extension_id; }); !kept.is_end()) {
        auto path = LexicalPath::join(packages_directory(), kept->file_name).string();
        if (auto result = FileSystem::remove(path, FileSystem::RecursionMode::Disallowed); result.is_error())
            warnln("Extensions: cannot remove {}: {}", path, result.error());
        m_kept_packages.remove_all_matching([&](auto const& kept) { return kept.extension_id == extension_id; });
        write_index();
    }

    outln("Removed extension {}", extension_id);
    announce_uninstalled(extension_id);
    return true;
}

void StoreInstaller::handle_call(Caller const& caller, String const& name, String const& args_json, Reply reply)
{
    auto parsed = JsonValue::from_string(args_json);
    JsonArray args;
    if (!parsed.is_error() && parsed.value().is_array())
        args = parsed.value().as_array();

    if (caller.store == "chrome"sv && name.starts_with_bytes("cws."sv))
        return handle_chrome_call(caller, name, args, move(reply));
    if (caller.store == "firefox"sv && name.starts_with_bytes("amo."sv))
        return handle_firefox_call(caller, name, args, move(reply));
    reply(false, "This store API is not available here."_string);
}

void StoreInstaller::handle_chrome_call(Caller const&, String const& name, JsonArray const& args, Reply reply)
{
    auto id = string_arg(args, 0).to_byte_string();

    if (name == "cws.getAll"sv)
        return reply(true, MUST(String::from_byte_string(m_manager.management_info().value_or("[]"sv))));

    if (name == "cws.get"sv) {
        if (auto info = is_installed(id) ? m_manager.management_info(id) : Optional<ByteString> {}; info.has_value())
            return reply(true, MUST(String::from_byte_string(*info)));
        return reply(false, MUST(String::formatted("Failed to find extension with id {}.", id)));
    }

    if (name == "cws.getExtensionStatus"sv) {
        auto status = is_installed(id) ? "enabled"sv : "installable"sv;
        return reply(true, json_string(status));
    }

    if (name == "cws.permissionWarningsByManifest"sv || name == "cws.permissionWarningsById"sv) {
        auto preview = name == "cws.permissionWarningsById"sv ? m_manager.preview_store_manifest({}, {}, id) : m_manager.preview_store_manifest(id, {});
        auto object = parse_object(preview);
        if (!object.has_value() || !object->get_bool("ok"sv).value_or(false))
            return reply(false, "Could not read the manifest."_string);
        JsonArray warnings;
        for (auto& warning : warnings_of(*object))
            warnings.must_append(warning);
        return reply(true, warnings.serialized());
    }

    if (name == "cws.beginInstall"sv)
        return chrome_begin_install(args, move(reply));

    if (name == "cws.completeInstall"sv)
        return chrome_complete_install(id, move(reply));

    if (name == "cws.setEnabled"sv) {
        // Installed extensions are always enabled; there is nothing to switch.
        if (!is_installed(id))
            return reply(false, MUST(String::formatted("Failed to find extension with id {}.", id)));
        return reply(true, String {});
    }

    if (name == "cws.uninstall"sv) {
        auto info = is_installed(id) ? m_manager.management_info(id) : Optional<ByteString> {};
        if (!info.has_value())
            return reply(false, MUST(String::formatted("Failed to find extension with id {}.", id)));
        auto confirm = args.size() > 1 && args.at(1).is_bool() && args.at(1).as_bool();
        if (!confirm) {
            uninstall(id);
            return reply(true, String {});
        }
        auto display_name = parse_object(*info).map([](auto const& object) { return object.get_string("name"sv).value_or({}); }).value_or({});
        Application::the().display_extension_prompt(
            { .title = MUST(String::formatted("Remove \"{}\"?", display_name)), .message = {}, .details = {}, .accept_label = "Remove"_string },
            [this, id, reply = move(reply)](bool accepted) {
                if (!accepted)
                    return reply(false, "User cancelled uninstall"_string);
                uninstall(id);
                reply(true, String {});
            });
        return;
    }

    reply(false, MUST(String::formatted("{} is not supported by this browser.", name)));
}

void StoreInstaller::chrome_begin_install(JsonArray const& args, Reply reply)
{
    auto result = [&](StringView code) { reply(true, json_string(code)); };
    if (args.is_empty() || !args.at(0).is_object())
        return result("unknown_error"sv);
    auto const& details = args.at(0).as_object();
    auto id = details.get_string("id"sv).value_or({}).to_byte_string();
    auto manifest = details.get_string("manifest"sv).value_or({}).to_byte_string();
    auto localized_name = details.get_string("localizedName"sv).value_or({});

    if (!ExtensionManager::chrome_web_store_download_url(id, reported_chrome_version()).has_value())
        return result("invalid_id"sv);
    if (m_installs_in_progress.contains(id))
        return result("install_in_progress"sv);

    auto preview = parse_object(m_manager.preview_store_manifest(manifest, localized_name));
    if (!preview.has_value() || !preview->get_bool("ok"sv).value_or(false))
        return result("manifest_error"sv);

    auto display_name = preview->get_string("name"sv).value_or({});
    if (display_name.is_empty())
        display_name = MUST(String::from_byte_string(id));
    auto warnings = warnings_of(*preview);
    auto is_update = is_installed(id);

    m_installs_in_progress.set(id);
    Application::the().display_extension_prompt(
        {
            .title = is_update ? MUST(String::formatted("Reinstall \"{}\" from the Chrome Web Store?", display_name)) : MUST(String::formatted("Add \"{}\"?", display_name)),
            .message = warnings.is_empty() ? String {} : "It can:"_string,
            .details = move(warnings),
            .accept_label = "Add extension"_string,
        },
        [this, id, manifest = move(manifest), reply = move(reply)](bool accepted) {
            if (!accepted) {
                m_installs_in_progress.remove(id);
                return reply(true, json_string("user_cancelled"sv));
            }
            m_approved_installs.set(id, manifest);
            reply(true, json_string(""sv));
        });
}

void StoreInstaller::chrome_complete_install(ByteString const& extension_id, Reply reply)
{
    auto approved = m_approved_installs.take(extension_id);
    if (!approved.has_value()) {
        m_installs_in_progress.remove(extension_id);
        return reply(false, "No install of this item was approved."_string);
    }

    auto url_string = ExtensionManager::chrome_web_store_download_url(extension_id, reported_chrome_version());
    auto url = url_string.has_value() ? URL::Parser::basic_parse(*url_string) : Optional<URL::URL> {};
    if (!url.has_value()) {
        m_installs_in_progress.remove(extension_id);
        return reply(false, "Invalid extension id."_string);
    }

    download(url.release_value(), [this, extension_id, approved_manifest = approved.release_value(), reply = move(reply)](ErrorOr<ByteBuffer> package) {
        ScopeGuard done = [&] { m_installs_in_progress.remove(extension_id); };
        if (package.is_error())
            return reply(false, MUST(String::formatted("{}", package.error())));

        auto check = parse_object(ExtensionManager::check_store_package(package.value().bytes(), extension_id, {}, approved_manifest));
        if (!check.has_value() || !check->get_bool("ok"sv).value_or(false)) {
            auto error = check.has_value() ? check->get_string("error"sv).value_or({}) : String {};
            warnln("Extensions: refusing the package of {} from the Chrome Web Store: {}", extension_id, error);
            return reply(false, MUST(String::formatted("Package is invalid: {}", error)));
        }

        auto was_installed = is_installed(extension_id);
        auto installed = install_and_keep(package.value().bytes(), "chrome"sv);
        if (installed.is_error())
            return reply(false, MUST(String::formatted("{}", installed.error())));

        if (on_installed)
            on_installed(installed.value(), was_installed);
        reply(true, String {});
        announce_installed(installed.value());
    });
}

void StoreInstaller::handle_firefox_call(Caller const& caller, String const& name, JsonArray const& args, Reply reply)
{
    auto id = string_arg(args, 0).to_byte_string();

    if (name == "amo.getAddonByID"sv)
        return reply(true, MUST(String::from_byte_string(id.is_empty() ? ByteString { "null"sv } : m_manager.amo_addon(id).value_or("null"sv))));

    if (name == "amo.install"sv) {
        if (args.size() < 2 || !args.at(0).is_integer<i64>())
            return reply(false, "Bad install request."_string);
        return firefox_install(caller, args.at(0).as_integer<i64>(), string_arg(args, 1), string_arg(args, 2), move(reply));
    }

    if (name == "amo.cancel"sv)
        return reply(true, String {});

    if (name == "amo.uninstall"sv)
        return reply(true, uninstall(id) ? "true"_string : "false"_string);

    if (name == "amo.setEnabled"sv)
        return reply(true, String {});

    reply(false, MUST(String::formatted("{} is not supported by this browser.", name)));
}

void StoreInstaller::firefox_install(Caller const& caller, i64 install_id, String const& url_string, String const& hash, Reply reply)
{
    auto context_id = caller.context_id;
    auto progress = [context_id, install_id](StringView event, StringView state, Optional<StringView> error = {}, Optional<size_t> size = {}) {
        JsonObject message;
        message.set("kind"sv, "install"sv);
        message.set("installId"sv, install_id);
        message.set("event"sv, MUST(String::from_utf8(event)));
        message.set("state"sv, MUST(String::from_utf8(state)));
        if (error.has_value())
            message.set("error"sv, MUST(String::from_utf8(*error)));
        if (size.has_value()) {
            message.set("progress"sv, static_cast<i64>(*size));
            message.set("maxProgress"sv, static_cast<i64>(*size));
        }
        auto serialized = message.serialized();
        deliver_to_extension_context(context_id, serialized.bytes_as_string_view());
    };

    auto url = URL::Parser::basic_parse(url_string);
    if (!url.has_value() || url->scheme() != "https"sv) {
        progress("onDownloadFailed"sv, "STATE_DOWNLOAD_FAILED"sv, "ERROR_INVALID_DOMAIN"sv);
        return reply(true, String {});
    }
    // AMO always gives a hash; without one, nothing would tie the file to the listing the user is looking at.
    if (!hash.starts_with_bytes("sha256:"sv)) {
        progress("onDownloadFailed"sv, "STATE_DOWNLOAD_FAILED"sv, "ERROR_INCORRECT_HASH"sv);
        return reply(true, String {});
    }

    progress("onDownloadStarted"sv, "STATE_DOWNLOADING"sv);
    download(url.release_value(), [this, progress, hash, reply = move(reply)](ErrorOr<ByteBuffer> package) mutable {
        if (package.is_error()) {
            progress("onDownloadFailed"sv, "STATE_DOWNLOAD_FAILED"sv, "ERROR_NETWORK_FAILURE"sv);
            return reply(true, String {});
        }
        auto size = package.value().size();
        progress("onDownloadProgress"sv, "STATE_DOWNLOADING"sv, {}, size);

        auto check = parse_object(ExtensionManager::check_store_package(package.value().bytes(), {}, hash, {}));
        if (!check.has_value() || !check->get_bool("ok"sv).value_or(false)) {
            auto error = check.has_value() ? check->get_string("error"sv).value_or({}) : String {};
            warnln("Extensions: refusing a package from addons.mozilla.org: {}", error);
            progress("onDownloadFailed"sv, "STATE_DOWNLOAD_FAILED"sv, error.contains("hash"sv) ? "ERROR_INCORRECT_HASH"sv : "ERROR_CORRUPT_FILE"sv);
            return reply(true, String {});
        }
        progress("onDownloadEnded"sv, "STATE_DOWNLOADED"sv, {}, size);

        auto extension_id = check->get_string("id"sv).value_or({}).to_byte_string();
        auto display_name = check->get_string("name"sv).value_or({});
        auto warnings = warnings_of(*check);
        auto is_update = is_installed(extension_id);
        Application::the().display_extension_prompt(
            {
                .title = is_update ? MUST(String::formatted("Reinstall \"{}\" from addons.mozilla.org?", display_name)) : MUST(String::formatted("Add \"{}\"?", display_name)),
                .message = warnings.is_empty() ? String {} : "It can:"_string,
                .details = move(warnings),
                .accept_label = "Add"_string,
            },
            [this, progress, package = package.release_value(), is_update, reply = move(reply)](bool accepted) {
                if (!accepted) {
                    progress("onInstallCancelled"sv, "STATE_CANCELLED"sv);
                    return reply(true, String {});
                }
                progress("onInstallStarted"sv, "STATE_INSTALLING"sv);
                auto installed = install_and_keep(package.bytes(), "firefox"sv);
                if (installed.is_error()) {
                    progress("onInstallFailed"sv, "STATE_INSTALL_FAILED"sv, "ERROR_CORRUPT_FILE"sv);
                    return reply(true, String {});
                }
                if (on_installed)
                    on_installed(installed.value(), is_update);
                progress("onInstallEnded"sv, "STATE_INSTALLED"sv);
                reply(true, String {});
                announce_installed(installed.value());
            });
    });
}

}

#endif
