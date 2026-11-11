/*
 * Copyright (c) 2026, cgx-webext contributors
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

#include <AK/HashMap.h>
#include <AK/JsonValue.h>
#include <LibCore/System.h>
#include <AK/Time.h>
#include <LibWeb/DOM/Document.h>
#include <LibWeb/HTML/LocalNavigable.h>
#include <LibWeb/Loader/ExtensionNetwork.h>
#include <LibWeb/Page/Page.h>

namespace Web::ExtensionNetwork {

static u32 s_interest = 0;
static u64 s_next_token = 1;
static u64 s_next_request = 1;

static HashMap<u64, Function<void(JsonObject)>>& waiting()
{
    static HashMap<u64, Function<void(JsonObject)>> s_waiting;
    return s_waiting;
}

static constexpr Array s_stages = {
    Stage::BeforeRequest,
    Stage::BeforeSendHeaders,
    Stage::SendHeaders,
    Stage::HeadersReceived,
    Stage::AuthRequired,
    Stage::BeforeRedirect,
    Stage::ResponseStarted,
    Stage::Completed,
    Stage::ErrorOccurred,
};

StringView stage_name(Stage stage)
{
    switch (stage) {
    case Stage::BeforeRequest:
        return "beforeRequest"sv;
    case Stage::BeforeSendHeaders:
        return "beforeSendHeaders"sv;
    case Stage::SendHeaders:
        return "sendHeaders"sv;
    case Stage::HeadersReceived:
        return "headersReceived"sv;
    case Stage::AuthRequired:
        return "authRequired"sv;
    case Stage::BeforeRedirect:
        return "beforeRedirect"sv;
    case Stage::ResponseStarted:
        return "responseStarted"sv;
    case Stage::Completed:
        return "completed"sv;
    case Stage::ErrorOccurred:
        return "errorOccurred"sv;
    }
    VERIFY_NOT_REACHED();
}

Optional<Stage> stage_from_name(StringView name)
{
    for (auto stage : s_stages) {
        if (stage_name(stage) == name)
            return stage;
    }
    return {};
}

void set_interest(u32 stages)
{
    s_interest = stages;
}

u32 interest()
{
    return s_interest;
}

String next_request_id()
{
    return MUST(String::formatted("{}.{}", Core::System::getpid(), s_next_request++));
}

void ask(Page& page, Stage stage, Optional<HTML::CrossProcessId> navigable_id, JsonObject const& details, Function<void(JsonObject)> on_answer)
{
    if (!is_interested(stage)) {
        on_answer({});
        return;
    }
    auto token = s_next_token++;
    waiting().set(token, move(on_answer));
    page.client().page_did_report_extension_network_stage(token, stage_name(stage), navigable_id, details.serialized());
}

void notify(Page& page, Stage stage, Optional<HTML::CrossProcessId> navigable_id, JsonObject const& details)
{
    if (is_interested(stage))
        page.client().page_did_report_extension_network_stage(0, stage_name(stage), navigable_id, details.serialized());
}

void notify_navigation(Page& page, HTML::CrossProcessId navigable_id, StringView event, URL::URL const& url, Optional<HTML::CrossProcessId> document_id)
{
    if ((s_interest & navigation_interest) == 0)
        return;
    JsonObject details;
    details.set("event"sv, event);
    details.set("url"sv, url.serialize());
    details.set("timeStamp"sv, UnixDateTime::now().milliseconds_since_epoch());
    if (document_id.has_value())
        details.set("documentId"sv, MUST(String::formatted("{}", document_id->local_id)));
    page.client().page_did_report_extension_network_stage(0, "navigation"sv, navigable_id, details.serialized());
}

void notify_navigation(DOM::Document& document, StringView event)
{
    if ((s_interest & navigation_interest) == 0)
        return;
    // Only a navigable's own document navigates; one without an id never was one.
    auto navigable = document.navigable();
    if (!navigable || !document.id_if_assigned().has_value())
        return;
    notify_navigation(document.page(), navigable->id(), event, document.url(), document.id_if_assigned());
}

void did_receive_answer(u64 token, StringView answer_json)
{
    auto on_answer = waiting().take(token);
    if (!on_answer.has_value())
        return;
    auto parsed = JsonValue::from_string(answer_json);
    (*on_answer)(!parsed.is_error() && parsed.value().is_object() ? parsed.value().as_object() : JsonObject {});
}

}
