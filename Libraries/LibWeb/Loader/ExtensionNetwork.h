/*
 * Copyright (c) 2026, cgx-webext contributors
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

#pragma once

#include <AK/Function.h>
#include <AK/JsonObject.h>
#include <AK/Optional.h>
#include <AK/String.h>
#include <LibWeb/Export.h>
#include <LibURL/URL.h>
#include <LibWeb/Forward.h>
#include <LibWebCommon/HTML/CrossProcessId.h>

// AD-HOC (cgx-webext): Lets browser extensions watch and change network requests (webRequest, declarativeNetRequest).
//
// The browser process tells each renderer which stages of a request any extension cares about. At those stages fetching
// describes the request to the browser process and, for the stages that can change it, waits for the answer.
namespace Web::ExtensionNetwork {

enum class Stage : u32 {
    BeforeRequest = 1 << 0,
    BeforeSendHeaders = 1 << 1,
    SendHeaders = 1 << 2,
    HeadersReceived = 1 << 3,
    AuthRequired = 1 << 4,
    BeforeRedirect = 1 << 5,
    ResponseStarted = 1 << 6,
    Completed = 1 << 7,
    ErrorOccurred = 1 << 8,
};

WEB_API StringView stage_name(Stage);
WEB_API Optional<Stage> stage_from_name(StringView);

// The stages to report, as a set of Stage bits. Zero (the default) means no extension is interested.
WEB_API void set_interest(u32 stages);
WEB_API u32 interest();
inline bool is_interested(Stage stage) { return (interest() & to_underlying(stage)) != 0; }

// An id for a request, unique among the browser's processes.
WEB_API String next_request_id();

// Reports a stage and waits for the browser process: `on_answer` gets its answer ({} to go on). `navigable_id` is the
// frame the request is made in, or the frame a navigation request loads into.
WEB_API void ask(Page&, Stage, Optional<HTML::CrossProcessId> navigable_id, JsonObject const& details, Function<void(JsonObject)> on_answer);
// Reports a stage nothing can be changed at.
WEB_API void notify(Page&, Stage, Optional<HTML::CrossProcessId> navigable_id, JsonObject const& details);

// Set in the interest when extensions want to hear about navigations (webNavigation).
inline constexpr u32 navigation_interest = 1u << 16;
// Reports a step of a document's life: "beforeNavigate", "committed", "domContentLoaded", "completed",
// "historyStateUpdated" or "referenceFragmentUpdated".
WEB_API void notify_navigation(Page&, HTML::CrossProcessId navigable_id, StringView event, URL::URL const&, Optional<HTML::CrossProcessId> document_id = {});
WEB_API void notify_navigation(DOM::Document&, StringView event);

// The browser process answered ask().
WEB_API void did_receive_answer(u64 token, StringView answer_json);

}
