/*
 * Copyright (c) 2026, cgx-webext contributors
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

#pragma once

#if defined(ENABLE_WEBEXT)

#    include <AK/ByteString.h>
#    include <AK/RefPtr.h>
#    include <LibCore/Timer.h>
#    include <LibWebView/Extensions/ExtensionManager.h>

namespace WebView {

// Connects the extension core to the rest of the browser: persistence under the profile directory, the alarm timer, and
// (as the integration grows) delivery of messages to extension pages and content scripts.
class ApplicationExtensionDelegate final : public ExtensionManager::Delegate {
    AK_ALLOC_WITH_KMALLOC;

public:
    explicit ApplicationExtensionDelegate(ByteString data_directory);

    void set_manager(ExtensionManager& manager) { m_manager = &manager; }

    virtual Optional<ByteString> read_extension_data(StringView extension_id, StringView key) override;
    virtual void write_extension_data(StringView extension_id, StringView key, StringView json) override;
    virtual void dispatch_event(StringView extension_id, StringView event, StringView args_json) override;
    virtual void deliver_to_context(u64 context_id, StringView envelope_json) override;
    virtual void call_browser(StringView call_json, Function<void(ErrorOr<ByteString>)> reply) override;
    virtual void request_permissions(StringView extension_id, StringView request_json, Function<void(bool)> reply) override;
    virtual void schedule_alarm_check(i64 at_ms) override;

private:
    ByteString path_for(StringView extension_id, StringView key) const;

    ByteString m_data_directory;
    ExtensionManager* m_manager { nullptr };
    RefPtr<Core::Timer> m_alarm_timer;
};

}

#endif
