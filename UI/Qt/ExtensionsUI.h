/*
 * Copyright (c) 2026, cgx-webext contributors
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

#pragma once

#if defined(ENABLE_WEBEXT)

#    include <AK/ByteString.h>
#    include <AK/Error.h>
#    include <AK/Function.h>
#    include <AK/JsonObject.h>
#    include <AK/StringView.h>
#    include <AK/Vector.h>
#    include <AK/kmalloc.h>

#    include <QPointer>
#    include <QWidget>

class QHBoxLayout;
class QToolButton;
class QShortcut;

namespace Ladybird {

class BrowserWindow;
class Tab;

// The extensions part of a window's toolbar, as in Chrome and Firefox: a button for each extension the user pinned, with
// its icon and badge, and the extensions button (a puzzle piece), which opens a panel listing every extension.
class ExtensionsToolbarArea final : public QWidget {
    Q_OBJECT

public:
    AK_ALLOC_WITH_KMALLOC;

    explicit ExtensionsToolbarArea(QWidget* parent);
    virtual ~ExtensionsToolbarArea() override;

    void set_tab(Tab*);
    // Redraws the buttons from the extension core's state for the shown tab.
    void refresh();

    // The button an extension's popup hangs from: its own, if pinned, else the extensions button.
    QWidget& anchor_for(ByteString const& extension_id);
    void show_panel();

private:
    BrowserWindow* browser_window() const;
    void rebuild_shortcuts();

    QHBoxLayout* m_buttons_layout { nullptr };
    QToolButton* m_puzzle_button { nullptr };
    Vector<QPointer<QToolButton>> m_action_buttons;
    Vector<QPointer<QShortcut>> m_shortcuts;
    QPointer<Tab> m_tab;
    ByteString m_last_state;
    ByteString m_last_commands;
};

namespace Extensions {

// What the Qt application does for the extension core (see WebView::Application's extension_* functions).
void host_call(ByteString const& extension_id, StringView function, JsonObject const& args, Function<void(ErrorOr<ByteString>)> reply);
void ui_changed(StringView extension_id, StringView what);
bool is_popup_view(u64 view_id);
void popup_request(u64 view_id, StringView name, StringView args_json);

// Starts reporting the browser's windows and tabs to the extension core.
void start_reporting_browser_state();
// Reports them now, if anything changed.
void report_browser_state();

// The user opened an extension (its toolbar button, its entry in the panel, or its shortcut): opens its popup, or tells it
// it was clicked.
void open_extension(BrowserWindow&, ByteString const& extension_id, QWidget* anchor);

}

}

#endif
