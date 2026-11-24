/*
 * Copyright (c) 2026, cgx-webext contributors
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

#if defined(ENABLE_WEBEXT)

#    include <AK/Base64.h>
#    include <AK/QuickSort.h>
#    include <AK/HashMap.h>
#    include <AK/JsonArray.h>
#    include <AK/JsonValue.h>
#    include <LibCore/File.h>
#    include <LibURL/Parser.h>
#    include <LibWebView/Application.h>
#    include <LibWebView/Extensions/ExtensionManager.h>
#    include <LibWebView/HistoryStore.h>
#    include <LibWebView/SearchEngine.h>
#    include <UI/Qt/Application.h>
#    include <UI/Qt/BookmarkStore.h>
#    include <UI/Qt/BrowserWindow.h>
#    include <UI/Qt/ChromeStyle.h>
#    include <UI/Qt/ExtensionsUI.h>
#    include <UI/Qt/HubConnection.h>
#    include <UI/Qt/Popover.h>
#    include <UI/Qt/StringUtils.h>
#    include <UI/Qt/Tab.h>
#    include <UI/Qt/WebContentView.h>
#    include <UI/Qt/WindowToolbar.h>

#    include <QApplication>
#    include <QBuffer>
#    include <QDateTime>
#    include <QFontDatabase>
#    include <QFrame>
#    include <QHBoxLayout>
#    include <QImage>
#    include <QKeyEvent>
#    include <QLabel>
#    include <QMenu>
#    include <QPainter>
#    include <QPainterPath>
#    include <QPushButton>
#    include <QScrollArea>
#    include <QShortcut>
#    include <QTimer>
#    include <QToolButton>
#    include <QVBoxLayout>

namespace Ladybird {

static constexpr int PANEL_WIDTH = 340;
static constexpr int PANEL_MAX_HEIGHT = 480;
static constexpr int ACTION_ICON_SIZE = 18;
static constexpr int ROW_ICON_SIZE = 20;
// Chrome's limits for a popup's size.
static constexpr int POPUP_MIN = 25;
static constexpr int POPUP_MAX_WIDTH = 800;
static constexpr int POPUP_MAX_HEIGHT = 600;

// The extension core lives in the Hub. Where the Hub runs in a process of its own, it is reached over IPC; otherwise
// directly.
static bool core_is_available()
{
#    if defined(LADYBIRD_HUB_IN_OWN_PROCESS)
    auto* hub = HubConnection::the_if_connected();
    return hub && hub->extension_core_available();
#    else
    return WebView::Application::extension_manager() != nullptr;
#    endif
}

static ByteString core_host_event(StringView name, StringView args_json)
{
#    if defined(LADYBIRD_HUB_IN_OWN_PROCESS)
    auto* hub = HubConnection::the_if_connected();
    return hub ? hub->extension_host_event(name, args_json) : ByteString {};
#    else
    auto* manager = WebView::Application::extension_manager();
    return manager ? manager->host_event(name, args_json) : ByteString {};
#    endif
}

static Optional<ByteBuffer> core_read_file(StringView extension_id, StringView path)
{
#    if defined(LADYBIRD_HUB_IN_OWN_PROCESS)
    auto* hub = HubConnection::the_if_connected();
    return hub ? hub->extension_read_file(extension_id, path) : Optional<ByteBuffer> {};
#    else
    auto* manager = WebView::Application::extension_manager();
    return manager ? manager->read_file(extension_id, path) : Optional<ByteBuffer> {};
#    endif
}

static Optional<ByteString> core_extension_for_url(StringView url)
{
#    if defined(LADYBIRD_HUB_IN_OWN_PROCESS)
    auto* hub = HubConnection::the_if_connected();
    return hub ? hub->extension_for_url(url) : Optional<ByteString> {};
#    else
    auto* manager = WebView::Application::extension_manager();
    return manager ? manager->extension_for_url(url) : Optional<ByteString> {};
#    endif
}

static void core_remove_extension(ByteString const& extension_id)
{
#    if defined(LADYBIRD_HUB_IN_OWN_PROCESS)
    if (auto* hub = HubConnection::the_if_connected())
        hub->async_extension_remove(extension_id);
#    else
    WebView::Application::the().remove_extension(extension_id);
#    endif
}

// Extension popups and the extensions panel hang below their button in a toolbar at the top of the window, and open
// above it when the button sits low in the window (the footer of the "everything in the sidebar" layout), like the
// sidebar's own popovers.
static void place_popover(Popover& popover, QWidget& anchor)
{
    auto* window = anchor.window();
    auto anchor_center = anchor.mapTo(window, anchor.rect().center());
    if (window && anchor_center.y() > window->height() / 2)
        move_popover_above(popover, anchor, PopoverAlignment::Leading);
    else
        move_popover_below(popover, anchor, PopoverAlignment::Trailing);
}

static JsonValue ask_core(StringView name, JsonObject const& args)
{
    if (!core_is_available())
        return JsonValue {};
    auto request = args.serialized();
    auto response = JsonValue::from_string(core_host_event(name, request.bytes_as_string_view()));
    return response.is_error() ? JsonValue {} : response.release_value();
}

// ---- Windows and tabs, as the extension core sees them --------------------------------------------------------------

static i64 window_id(BrowserWindow& window)
{
    static i64 s_next_id = 1;
    auto value = window.property("cgxWindowId");
    if (value.isValid())
        return value.toLongLong();
    auto id = s_next_id++;
    window.setProperty("cgxWindowId", QVariant::fromValue<qlonglong>(id));
    return id;
}

static Vector<BrowserWindow*> browser_windows()
{
    Vector<BrowserWindow*> windows;
    for (auto* widget : QApplication::topLevelWidgets()) {
        if (auto* window = qobject_cast<BrowserWindow*>(widget))
            windows.append(window);
    }
    quick_sort(windows, [](auto* a, auto* b) { return window_id(*a) < window_id(*b); });
    return windows;
}

static BrowserWindow* window_with_id(Optional<i64> id)
{
    if (!id.has_value())
        return Application::the().active_window_if_any();
    for (auto* window : browser_windows()) {
        if (window_id(*window) == *id)
            return window;
    }
    return nullptr;
}

struct TabPlace {
    BrowserWindow* window { nullptr };
    Tab* tab { nullptr };
    int index { 0 };
};

static Optional<TabPlace> find_tab(i64 tab_id)
{
    for (auto* window : browser_windows()) {
        for (int i = 0; i < window->tab_count(); ++i) {
            auto* tab = window->tab_at(i);
            if (tab && tab->is_loaded() && static_cast<i64>(tab->view().view_id()) == tab_id)
                return TabPlace { window, tab, i };
        }
    }
    return {};
}

static i64 tab_id(Tab& tab)
{
    return tab.is_loaded() ? static_cast<i64>(tab.view().view_id()) : -1;
}

// The window the user is in: the browser window itself, or the one an extension panel or popup belongs to.
static BrowserWindow* focused_window()
{
    for (auto* widget = QApplication::activeWindow(); widget; widget = widget->parentWidget()) {
        if (auto* window = qobject_cast<BrowserWindow*>(widget->window()))
            return window;
        if (!widget->parentWidget())
            break;
    }
    return nullptr;
}

static ByteString browser_state_json()
{
    JsonArray windows;
    auto* focused = focused_window();
    for (auto* window : browser_windows()) {
        JsonObject object;
        object.set("id"sv, window_id(*window));
        object.set("type"sv, "normal"sv);
        auto state = window->isMinimized() ? "minimized"sv : window->isFullScreen() ? "fullscreen"sv
            : window->isMaximized()                                                 ? "maximized"sv
                                                                                    : "normal"sv;
        object.set("state"sv, state);
        object.set("focused"sv, window == focused);
        object.set("incognito"sv, window->is_private() == WebView::IsPrivate::Yes);
        auto geometry = window->frameGeometry();
        object.set("left"sv, geometry.x());
        object.set("top"sv, geometry.y());
        object.set("width"sv, geometry.width());
        object.set("height"sv, geometry.height());

        JsonArray tabs;
        for (int i = 0; i < window->tab_count(); ++i) {
            auto* tab = window->tab_at(i);
            // A restored tab that has not loaded yet has no page, so nothing an extension could act on.
            if (!tab || !tab->is_loaded())
                continue;
            auto& view = tab->view();
            JsonObject entry;
            entry.set("id"sv, tab_id(*tab));
            entry.set("active"sv, tab == window->current_tab());
            entry.set("pinned"sv, tab->is_pinned());
            entry.set("url"sv, view.url().serialize());
            entry.set("title"sv, ak_string_from_qstring(tab->title()));
            entry.set("status"sv, tab->is_loading() ? "loading"sv : "complete"sv);
            entry.set("muted"sv, view.page_mute_state() == Web::HTML::MuteState::Muted);
            entry.set("zoom"sv, view.zoom_level());
            entry.set("width"sv, view.width());
            entry.set("height"sv, view.height());
            MUST(tabs.append(move(entry)));
        }
        object.set("tabs"sv, move(tabs));
        MUST(windows.append(move(object)));
    }
    JsonObject root;
    root.set("windows"sv, move(windows));
    if (auto* last = Application::the().active_window_if_any())
        root.set("lastFocusedWindowId"sv, window_id(*last));
    return root.serialized().to_byte_string();
}

namespace Extensions {

void report_browser_state()
{
    static ByteString s_last_reported;
    if (!core_is_available())
        return;
    auto state = browser_state_json();
    if (state == s_last_reported)
        return;
    s_last_reported = state;
    (void)core_host_event("browserState"sv, state);
}

void start_reporting_browser_state()
{
    if (!core_is_available())
        return;
    // Tabs change in many places (navigation, titles, loading, dragging); comparing a small snapshot a few times a second
    // catches all of them, and only a change reaches the extensions.
    auto* timer = new QTimer(qApp);
    timer->setInterval(250);
    QObject::connect(timer, &QTimer::timeout, [] { report_browser_state(); });
    timer->start();
    report_browser_state();
}

}

// ---- Icons ----------------------------------------------------------------------------------------------------------

static QPixmap pixmap_from_core_icon(ByteString const& extension_id, JsonValue const& icon, int pixels)
{
    if (!icon.is_object())
        return {};
    auto const& object = icon.as_object();
    auto kind = object.get_string("kind"sv).value_or({});
    // The smallest image at least as big as wanted, else the biggest.
    auto pick = [&](JsonArray const& list, StringView size_key) -> Optional<JsonObject> {
        Optional<JsonObject> best;
        i64 best_size = 0;
        list.for_each([&](JsonValue const& entry) {
            if (!entry.is_object())
                return;
            auto size = entry.as_object().get_integer<i64>(size_key).value_or(0);
            auto better = !best.has_value()
                || (size >= pixels && (best_size < pixels || size < best_size))
                || (size < pixels && best_size < pixels && size > best_size);
            if (better) {
                best = entry.as_object();
                best_size = size;
            }
        });
        return best;
    };
    if (kind == "paths"sv) {
        auto paths = object.get_array("paths"sv);
        if (!paths.has_value())
            return {};
        auto chosen = pick(*paths, "size"sv);
        if (!chosen.has_value())
            return {};
        auto path = chosen->get_string("path"sv).value_or({});
        auto bytes = core_read_file(extension_id, path);
        QPixmap pixmap;
        if (bytes.has_value())
            pixmap.loadFromData(reinterpret_cast<uchar const*>(bytes->data()), bytes->size());
        return pixmap;
    }
    if (kind == "pixels"sv) {
        auto images = object.get_array("images"sv);
        if (!images.has_value())
            return {};
        auto chosen = pick(*images, "size"sv);
        if (!chosen.has_value())
            return {};
        auto width = chosen->get_integer<int>("width"sv).value_or(0);
        auto height = chosen->get_integer<int>("height"sv).value_or(0);
        auto rgba = decode_base64(chosen->get_string("rgba"sv).value_or({}));
        if (rgba.is_error() || width <= 0 || height <= 0 || rgba.value().size() != static_cast<size_t>(width) * height * 4)
            return {};
        QImage image(reinterpret_cast<uchar const*>(rgba.value().data()), width, height, width * 4, QImage::Format_RGBA8888);
        return QPixmap::fromImage(image.copy());
    }
    return {};
}

// A letter on a tile, for an extension without an icon.
static QPixmap letter_tile(QString const& name, int logical, qreal dpr, QPalette const& palette)
{
    QPixmap pixmap(QSize(logical, logical) * dpr);
    pixmap.setDevicePixelRatio(dpr);
    pixmap.fill(Qt::transparent);
    QPainter painter(&pixmap);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setBrush(palette.color(QPalette::Highlight));
    painter.setPen(Qt::NoPen);
    painter.drawRoundedRect(QRectF(0, 0, logical, logical), 4, 4);
    painter.setPen(palette.color(QPalette::HighlightedText));
    auto font = painter.font();
    font.setPixelSize(logical * 3 / 5);
    font.setBold(true);
    painter.setFont(font);
    painter.drawText(QRectF(0, 0, logical, logical), Qt::AlignCenter, name.left(1).toUpper());
    return pixmap;
}

static QColor color_from(JsonValue const& value, QColor fallback)
{
    if (!value.is_array() || value.as_array().size() != 4)
        return fallback;
    auto const& c = value.as_array();
    auto component = [&](size_t i) { return static_cast<int>(c.at(i).get_integer<i64>().value_or(0)); };
    QColor color(component(0), component(1), component(2), component(3));
    return color.alpha() == 0 ? fallback : color;
}

// The extension's icon with its badge, as Chrome draws it: the text on a rounded label over the icon's lower edge.
static QIcon action_icon(ByteString const& extension_id, JsonObject const& extension, int logical, qreal dpr, QPalette const& palette)
{
    auto const& action = extension.get_object("action"sv);
    auto name = qstring_from_ak_string(extension.get_string("name"sv).value_or({}));
    auto source = action.has_value() ? pixmap_from_core_icon(extension_id, action->get("icon"sv).value_or(JsonValue {}), static_cast<int>(logical * dpr)) : QPixmap {};
    if (source.isNull()) {
        // The extension's own icons, when its button has none.
        JsonObject paths;
        paths.set("kind"sv, "paths"sv);
        paths.set("paths"sv, extension.get_array("icons"sv).value_or(JsonArray {}));
        source = pixmap_from_core_icon(extension_id, paths, static_cast<int>(logical * dpr));
    }
    QPixmap canvas(QSize(logical, logical) * dpr);
    canvas.setDevicePixelRatio(dpr);
    canvas.fill(Qt::transparent);
    QPainter painter(&canvas);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setRenderHint(QPainter::SmoothPixmapTransform);
    if (source.isNull())
        painter.drawPixmap(0, 0, letter_tile(name, logical, dpr, palette));
    else
        painter.drawPixmap(QRectF(0, 0, logical, logical), source, QRectF(source.rect()));

    auto badge = action.has_value() ? qstring_from_ak_string(action->get_string("badgeText"sv).value_or({})) : QString {};
    if (!badge.isEmpty()) {
        badge = badge.left(4);
        auto font = painter.font();
        font.setPixelSize(qMax(8, logical / 2));
        font.setBold(true);
        painter.setFont(font);
        QFontMetricsF metrics(font);
        auto width = qMin<qreal>(logical, metrics.horizontalAdvance(badge) + 4);
        auto height = metrics.height();
        QRectF rect(logical - width, logical - height, width, height);
        painter.setPen(Qt::NoPen);
        painter.setBrush(color_from(action->get("badgeBackgroundColor"sv).value_or(JsonValue {}), QColor(95, 99, 104)));
        painter.drawRoundedRect(rect, 2, 2);
        painter.setPen(color_from(action->get("badgeTextColor"sv).value_or(JsonValue {}), Qt::white));
        painter.drawText(rect, Qt::AlignCenter, badge);
    }
    painter.end();
    QIcon icon(canvas);
    return icon;
}

static QIcon puzzle_icon(QPalette const& palette)
{
    QIcon icon;
    for (int dpr : { 1, 2, 3 }) {
        QPixmap pixmap(QSize(ACTION_ICON_SIZE, ACTION_ICON_SIZE) * dpr);
        pixmap.setDevicePixelRatio(dpr);
        pixmap.fill(Qt::transparent);
        QPainter painter(&pixmap);
        painter.setRenderHint(QPainter::Antialiasing);
        // A puzzle piece: a square with a knob on top and on the right.
        QPainterPath path;
        path.addRoundedRect(QRectF(2.5, 6.5, 10, 10), 1.5, 1.5);
        path.addEllipse(QPointF(7.5, 5), 2.5, 2.5);
        path.addEllipse(QPointF(14, 11.5), 2.5, 2.5);
        painter.setPen(QPen(palette.color(QPalette::ButtonText), 1.4));
        painter.setBrush(Qt::NoBrush);
        painter.drawPath(path.simplified());
        painter.end();
        icon.addPixmap(pixmap);
    }
    return icon;
}

static QIcon pin_icon(QPalette const& palette, bool pinned)
{
    QPixmap pixmap(QSize(16, 16) * 2);
    pixmap.setDevicePixelRatio(2);
    pixmap.fill(Qt::transparent);
    QPainter painter(&pixmap);
    painter.setRenderHint(QPainter::Antialiasing);
    auto color = pinned ? palette.color(QPalette::Highlight) : palette.color(QPalette::ButtonText);
    painter.setPen(QPen(color, 1.3));
    painter.setBrush(pinned ? QBrush(color) : Qt::NoBrush);
    QPainterPath head;
    head.moveTo(5, 2.5);
    head.lineTo(11, 2.5);
    head.lineTo(10, 7);
    head.lineTo(12, 9.5);
    head.lineTo(4, 9.5);
    head.lineTo(6, 7);
    head.closeSubpath();
    painter.drawPath(head);
    painter.drawLine(QPointF(8, 9.5), QPointF(8, 14));
    painter.end();
    return QIcon(pixmap);
}

static QIcon more_icon(QPalette const& palette)
{
    QPixmap pixmap(QSize(16, 16) * 2);
    pixmap.setDevicePixelRatio(2);
    pixmap.fill(Qt::transparent);
    QPainter painter(&pixmap);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setPen(Qt::NoPen);
    painter.setBrush(palette.color(QPalette::ButtonText));
    for (int y : { 4, 8, 12 })
        painter.drawEllipse(QPointF(8, y), 1.4, 1.4);
    painter.end();
    return QIcon(pixmap);
}

// ---- The extension's popup ------------------------------------------------------------------------------------------

class ExtensionPopup final : public Popover {
public:
    AK_ALLOC_WITH_KMALLOC;

    ExtensionPopup(QWidget* parent, BrowserWindow& window, ByteString extension_id, URL::URL const& url, QWidget& anchor)
        : Popover(parent)
        , m_extension_id(AK::move(extension_id))
        , m_anchor(&anchor)
    {
        card().setObjectName("LadybirdDownloadsPopover");
        auto* layout = new QVBoxLayout(&card());
        layout->setContentsMargins(1, 1, 1, 1);
        layout->setSpacing(0);
        m_view = new WebContentView(&card(), {}, WebContentViewInitialState { .is_private = window.is_private(), .maximum_frames_per_second = 60.0, .display_id = {}, .owner_view_id = {} });
        m_view->setAccessibleName("Extension popup");
        layout->addWidget(m_view);
        s_popups.set(m_view->view_id(), this);
#    if defined(LADYBIRD_HUB_IN_OWN_PROCESS)
        if (auto* hub = HubConnection::the_if_connected())
            hub->async_extension_set_popup_view(m_view->view_id(), true);
#    endif
        set_content_size(POPUP_MIN * 8, POPUP_MIN * 4);
        on_visibility_change = [this](bool visible) {
            if (!visible) {
                m_closed_at = QDateTime::currentMSecsSinceEpoch();
                s_last_closed = { m_extension_id, m_closed_at };
                deleteLater();
            }
        };
        m_view->load(url);
    }

    virtual ~ExtensionPopup() override
    {
        if (m_view) {
            s_popups.remove(m_view->view_id());
#    if defined(LADYBIRD_HUB_IN_OWN_PROCESS)
            if (auto* hub = HubConnection::the_if_connected())
                hub->async_extension_set_popup_view(m_view->view_id(), false);
#    endif
        }
    }

    ByteString const& extension_id() const { return m_extension_id; }

    void set_content_size(int width, int height)
    {
        width = clamp(width, POPUP_MIN, POPUP_MAX_WIDTH);
        height = clamp(height, POPUP_MIN, POPUP_MAX_HEIGHT);
        if (m_view->width() == width && m_view->height() == height)
            return;
        m_view->setFixedSize(width, height);
        card().adjustSize();
        adjustSize();
        if (m_anchor)
            place_popover(*this, *m_anchor);
    }

    // Shown once the page has loaded and sized itself, as in Chrome, so that it does not flash empty; at the latest after
    // a few seconds, so that a slow page still opens.
    void open()
    {
        QTimer::singleShot(3000, this, [this] { reveal(); });
    }

    void reveal()
    {
        if (m_revealed)
            return;
        m_revealed = true;
        show();
        if (m_anchor)
            place_popover(*this, *m_anchor);
        raise();
        m_view->setFocus(Qt::PopupFocusReason);
    }

    static HashMap<u64, QPointer<ExtensionPopup>> s_popups;
    struct LastClosed {
        ByteString extension_id;
        qint64 at { 0 };
    };
    static LastClosed s_last_closed;

private:
    virtual void keyPressEvent(QKeyEvent* event) override
    {
        if (event->key() == Qt::Key_Escape) {
            close();
            return;
        }
        Popover::keyPressEvent(event);
    }

    ByteString m_extension_id;
    QPointer<QWidget> m_anchor;
    WebContentView* m_view { nullptr };
    qint64 m_closed_at { 0 };
    bool m_revealed { false };
};

HashMap<u64, QPointer<ExtensionPopup>> ExtensionPopup::s_popups;
ExtensionPopup::LastClosed ExtensionPopup::s_last_closed;

// One popup per window, as in Chrome.
static QPointer<ExtensionPopup>& popup_of(BrowserWindow& window)
{
    static HashMap<BrowserWindow*, QPointer<ExtensionPopup>> s_by_window;
    return s_by_window.ensure(&window);
}

static ExtensionsToolbarArea* toolbar_area_of(BrowserWindow& window)
{
    return window.toolbar().extensions_area();
}

static void show_popup(BrowserWindow& window, ByteString const& extension_id, URL::URL const& url, QWidget* anchor)
{
    auto& slot = popup_of(window);
    if (slot) {
        // A popup still loading was never shown, so closing it does not delete it.
        slot->close();
        slot->deleteLater();
    }
    if (!anchor) {
        auto* area = toolbar_area_of(window);
        anchor = area ? &area->anchor_for(extension_id) : static_cast<QWidget*>(&window);
    }
    auto* popup = new ExtensionPopup(anchor, window, extension_id, url, *anchor);
    slot = popup;
    popup->open();
}

namespace Extensions {

void open_extension(BrowserWindow& window, ByteString const& extension_id, QWidget* anchor)
{
    // Pressing the button of an open popup closes it (the press outside the popup); that press must not open it again.
    auto const& last = ExtensionPopup::s_last_closed;
    if (last.extension_id == extension_id && QDateTime::currentMSecsSinceEpoch() - last.at < 300)
        return;
    report_browser_state();
    JsonObject args;
    args.set("extensionId"sv, MUST(String::from_byte_string(extension_id)));
    if (auto* tab = window.current_tab(); tab && tab->is_loaded())
        args.set("tabId"sv, tab_id(*tab));
    auto result = ask_core("actionClicked"sv, args);
    if (!result.is_object())
        return;
    if (auto popup = result.as_object().get_string("popup"sv); popup.has_value()) {
        if (auto url = URL::Parser::basic_parse(*popup); url.has_value())
            show_popup(window, extension_id, *url, anchor);
    }
}

bool is_popup_view(u64 view_id)
{
    return ExtensionPopup::s_popups.contains(view_id);
}

void popup_request(u64 view_id, StringView name, StringView args_json)
{
    auto popup = ExtensionPopup::s_popups.get(view_id);
    if (!popup.has_value() || !*popup)
        return;
    if (name == "__ui.closePopup"sv) {
        (*popup)->close();
        return;
    }
    if (name == "__ui.popupReady"sv) {
        (*popup)->reveal();
        return;
    }
    if (name == "__ui.popupSize"sv) {
        auto args = JsonValue::from_string(args_json);
        if (args.is_error() || !args.value().is_array() || args.value().as_array().size() < 2)
            return;
        auto const& size = args.value().as_array();
        (*popup)->set_content_size(size.at(0).get_integer<int>().value_or(POPUP_MIN), size.at(1).get_integer<int>().value_or(POPUP_MIN));
    }
}

}

// ---- The extensions panel -------------------------------------------------------------------------------------------

static StringView site_access_text(StringView access)
{
    if (access == "granted"sv)
        return "Can read and change this site"sv;
    if (access == "on_click"sv)
        return "Can access this site when you click it"sv;
    return "Cannot access this site"sv;
}

// One extension in the panel: the whole row opens it, from the mouse or the keyboard.
class ExtensionRow final : public QFrame {
public:
    AK_ALLOC_WITH_KMALLOC;

    explicit ExtensionRow(QWidget* parent)
        : QFrame(parent)
    {
        setObjectName("LadybirdDownloadRow");
        setFocusPolicy(Qt::StrongFocus);
        setCursor(Qt::PointingHandCursor);
    }

    Function<void()> on_open;

private:
    virtual void mouseReleaseEvent(QMouseEvent* event) override
    {
        if (event->button() == Qt::LeftButton && rect().contains(event->position().toPoint()) && on_open)
            on_open();
        QFrame::mouseReleaseEvent(event);
    }
    virtual void keyPressEvent(QKeyEvent* event) override
    {
        if ((event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter || event->key() == Qt::Key_Space) && on_open) {
            on_open();
            return;
        }
        QFrame::keyPressEvent(event);
    }
};

class ExtensionsPanel final : public Popover {
public:
    AK_ALLOC_WITH_KMALLOC;

    ExtensionsPanel(QWidget* parent, BrowserWindow& window, QWidget& anchor)
        : Popover(parent)
        , m_window(&window)
        , m_anchor(&anchor)
    {
        card().setObjectName("LadybirdDownloadsPopover");
        card().setFixedWidth(PANEL_WIDTH);
        setStyleSheet(ChromeStyle::downloads_popover_style_sheet(palette()));
        m_layout = new QVBoxLayout(&card());
        m_layout->setContentsMargins(6, 12, 6, 6);
        m_layout->setSpacing(0);
        rebuild();
    }

    void rebuild()
    {
        // Remove the old rows.
        while (auto* item = m_layout->takeAt(0)) {
            if (auto* widget = item->widget())
                widget->deleteLater();
            if (auto* layout = item->layout()) {
                while (auto* inner = layout->takeAt(0)) {
                    if (inner->widget())
                        inner->widget()->deleteLater();
                    delete inner;
                }
            }
            delete item;
        }

        auto* title = new QLabel("Extensions", &card());
        title->setObjectName("LadybirdDownloadsPopoverTitle");
        title->setContentsMargins(10, 0, 4, 6);
        m_layout->addWidget(title);

        JsonObject args;
        auto* tab = m_window ? m_window->current_tab() : nullptr;
        if (tab && tab->is_loaded())
            args.set("tabId"sv, tab_id(*tab));
        auto state = ask_core("uiState"sv, args);
        auto extensions = state.is_array() ? state.as_array() : JsonArray {};

        if (extensions.is_empty()) {
            auto* empty = new QLabel("No extensions installed yet.\nAdd them from the Chrome Web Store or Firefox Add-ons.", &card());
            empty->setObjectName("LadybirdDownloadsEmpty");
            empty->setAlignment(Qt::AlignCenter);
            empty->setWordWrap(true);
            empty->setMinimumHeight(72);
            m_layout->addWidget(empty);
        } else {
            auto* scroll = new QScrollArea(&card());
            scroll->setObjectName("LadybirdDownloadsPopoverScroll");
            scroll->setFrameShape(QFrame::NoFrame);
            scroll->setWidgetResizable(true);
            scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
            auto* rows = new QWidget(scroll);
            rows->setObjectName("LadybirdDownloadsPopoverRows");
            auto* rows_layout = new QVBoxLayout(rows);
            rows_layout->setContentsMargins(0, 0, 0, 0);
            rows_layout->setSpacing(0);
            extensions.for_each([&](JsonValue const& value) {
                if (value.is_object())
                    rows_layout->addWidget(create_row(*rows, value.as_object()));
            });
            scroll->setWidget(rows);
            scroll->setMaximumHeight(PANEL_MAX_HEIGHT - 80);
            scroll->setMinimumHeight(qMin(PANEL_MAX_HEIGHT - 80, rows->sizeHint().height()));
            m_layout->addWidget(scroll);
        }

        auto* separator = new QFrame(&card());
        separator->setObjectName("LadybirdPopoverSeparator");
        separator->setFixedHeight(1);
        auto* separator_row = new QHBoxLayout;
        separator_row->setContentsMargins(10, 4, 10, 4);
        separator_row->addWidget(separator);
        m_layout->addLayout(separator_row);

        auto* find_more = new QPushButton("Find more extensions", &card());
        find_more->setObjectName("LadybirdPopoverRow");
        auto* stores = new QMenu(find_more);
        auto open_store = [this](StringView url) {
            close();
            if (auto parsed = URL::Parser::basic_parse(url); parsed.has_value() && m_window)
                m_window->new_tab_from_url(*parsed, Web::HTML::ActivateTab::Yes, BrowserWindow::TabLocation::after_current_tab());
        };
        QObject::connect(stores->addAction("Chrome Web Store"), &QAction::triggered, [open_store] { open_store("https://chromewebstore.google.com/category/extensions"sv); });
        QObject::connect(stores->addAction("Firefox Add-ons"), &QAction::triggered, [open_store] { open_store("https://addons.mozilla.org/firefox/extensions/"sv); });
        find_more->setMenu(stores);
        m_layout->addWidget(find_more);

        card().adjustSize();
        adjustSize();
        if (m_anchor)
            place_popover(*this, *m_anchor);
    }

private:
    QWidget* create_row(QWidget& parent, JsonObject const& extension)
    {
        auto id = extension.get_string("id"sv).value_or({}).to_byte_string();
        auto name = qstring_from_ak_string(extension.get_string("name"sv).value_or({}));
        auto const& action = extension.get_object("action"sv);
        auto pinned = action.has_value() && action->get_bool("pinned"sv).value_or(false);
        auto has_action = action.has_value() && action->get_string("kind"sv).value_or({}) != "none"sv;
        auto access = site_access_text(extension.get_string("siteAccess"sv).value_or({}));

        auto* row = new ExtensionRow(&parent);
        row->setAccessibleName(name);
        row->setAccessibleDescription(qstring_from_ak_string(access));
        auto* layout = new QHBoxLayout(row);
        layout->setContentsMargins(10, 6, 4, 6);
        layout->setSpacing(10);

        auto* icon = new QLabel(row);
        icon->setPixmap(action_icon(id, extension, ROW_ICON_SIZE, devicePixelRatioF(), palette()).pixmap(ROW_ICON_SIZE, ROW_ICON_SIZE));
        layout->addWidget(icon, 0, Qt::AlignVCenter);

        auto* texts = new QVBoxLayout;
        texts->setSpacing(1);
        auto* name_label = new QLabel(name, row);
        name_label->setObjectName("LadybirdDownloadFileName");
        auto* status_label = new QLabel(qstring_from_ak_string(access), row);
        status_label->setObjectName("LadybirdDownloadStatus");
        texts->addWidget(name_label);
        texts->addWidget(status_label);
        layout->addLayout(texts, 1);

        row->on_open = [this, id] {
            auto window = m_window;
            auto anchor = m_anchor;
            close();
            if (window)
                Extensions::open_extension(*window, id, anchor ? anchor.data() : nullptr);
        };

        if (has_action) {
            auto* pin = new QToolButton(row);
            pin->setAutoRaise(true);
            pin->setIcon(pin_icon(palette(), pinned));
            pin->setToolTip(pinned ? "Unpin from toolbar" : "Pin to toolbar");
            pin->setAccessibleName(pin->toolTip());
            QObject::connect(pin, &QToolButton::clicked, [this, id, pinned] {
                JsonObject args;
                args.set("extensionId"sv, MUST(String::from_byte_string(id)));
                args.set("pinned"sv, !pinned);
                (void)ask_core("setActionPinned"sv, args);
                rebuild();
            });
            layout->addWidget(pin, 0, Qt::AlignVCenter);
        }

        auto* more = new QToolButton(row);
        more->setAutoRaise(true);
        more->setIcon(more_icon(palette()));
        more->setToolTip("More actions");
        more->setAccessibleName(QString("More actions for %1").arg(name));
        more->setPopupMode(QToolButton::InstantPopup);
        auto* menu = new QMenu(more);
        if (auto options = extension.get_string("optionsUrl"sv); options.has_value()) {
            QObject::connect(menu->addAction("Options"), &QAction::triggered, [this, url = *options] {
                close();
                if (auto parsed = URL::Parser::basic_parse(url); parsed.has_value() && m_window)
                    m_window->new_tab_from_url(*parsed, Web::HTML::ActivateTab::Yes, BrowserWindow::TabLocation::after_current_tab());
            });
        }
        auto* access_action = menu->addAction(qstring_from_ak_string(access));
        access_action->setEnabled(false);
        menu->addSeparator();
        QObject::connect(menu->addAction("Remove from Ladybird…"), &QAction::triggered, [this, id] {
            close();
            core_remove_extension(id);
        });
        more->setMenu(menu);
        layout->addWidget(more, 0, Qt::AlignVCenter);
        return row;
    }

    QPointer<BrowserWindow> m_window;
    QPointer<QWidget> m_anchor;
    QVBoxLayout* m_layout { nullptr };
};

static QPointer<ExtensionsPanel>& open_panel()
{
    static QPointer<ExtensionsPanel> s_panel;
    return s_panel;
}

// ---- The toolbar ----------------------------------------------------------------------------------------------------

ExtensionsToolbarArea::ExtensionsToolbarArea(QWidget* parent)
    : QWidget(parent)
{
    auto* layout = new QHBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(2);
    m_buttons_layout = new QHBoxLayout;
    m_buttons_layout->setContentsMargins(0, 0, 0, 0);
    m_buttons_layout->setSpacing(2);
    layout->addLayout(m_buttons_layout);

    m_puzzle_button = new QToolButton(this);
    m_puzzle_button->setAutoRaise(true);
    m_puzzle_button->setIcon(puzzle_icon(palette()));
    m_puzzle_button->setIconSize({ ACTION_ICON_SIZE, ACTION_ICON_SIZE });
    m_puzzle_button->setToolTip("Extensions");
    m_puzzle_button->setAccessibleName("Extensions");
    QObject::connect(m_puzzle_button, &QToolButton::clicked, this, [this] { show_panel(); });
    layout->addWidget(m_puzzle_button);

    setVisible(core_is_available());
    static bool s_reporting = false;
    if (!s_reporting && core_is_available()) {
        s_reporting = true;
        // Not from here: the window that holds this toolbar is still being built and has no tabs yet.
        QTimer::singleShot(0, qApp, [] { Extensions::start_reporting_browser_state(); });
    }
    QTimer::singleShot(0, this, [this] { refresh(); });
}

ExtensionsToolbarArea::~ExtensionsToolbarArea() = default;

BrowserWindow* ExtensionsToolbarArea::browser_window() const
{
    return qobject_cast<BrowserWindow*>(window());
}

void ExtensionsToolbarArea::set_tab(Tab* tab)
{
    if (m_tab == tab)
        return;
    m_tab = tab;
    refresh();
}

QWidget& ExtensionsToolbarArea::anchor_for(ByteString const& extension_id)
{
    for (auto& button : m_action_buttons) {
        if (button && button->property("cgxExtensionId").toString() == qstring_from_ak_string(extension_id) && button->isVisible())
            return *button;
    }
    return *m_puzzle_button;
}

void ExtensionsToolbarArea::show_panel()
{
    auto* window = browser_window();
    if (!window)
        return;
    if (open_panel()) {
        open_panel()->close();
        return;
    }
    Extensions::report_browser_state();
    auto* panel = new ExtensionsPanel(this, *window, *m_puzzle_button);
    panel->on_visibility_change = [panel](bool visible) {
        if (!visible)
            panel->deleteLater();
    };
    open_panel() = panel;
    panel->show();
    place_popover(*panel, *m_puzzle_button);
    panel->raise();
}

void ExtensionsToolbarArea::refresh()
{
    if (!core_is_available())
        return;
    JsonObject args;
    if (m_tab && m_tab->is_loaded())
        args.set("tabId"sv, tab_id(*m_tab));
    auto state = ask_core("uiState"sv, args);
    auto serialized = state.serialized().to_byte_string();
    rebuild_shortcuts();
    if (serialized == m_last_state)
        return;
    m_last_state = serialized;

    for (auto& button : m_action_buttons) {
        if (button)
            button->deleteLater();
    }
    m_action_buttons.clear();

    auto extensions = state.is_array() ? state.as_array() : JsonArray {};
    auto count = extensions.size();
    auto label = count == 0 ? QString("Extensions") : QString("Extensions (%1 installed)").arg(count);
    m_puzzle_button->setToolTip(label);
    m_puzzle_button->setAccessibleName(label);

    extensions.for_each([&](JsonValue const& value) {
        if (!value.is_object())
            return;
        auto const& extension = value.as_object();
        auto const& action = extension.get_object("action"sv);
        if (!action.has_value() || action->get_string("kind"sv).value_or({}) == "none"sv || !action->get_bool("pinned"sv).value_or(false))
            return;
        auto id = extension.get_string("id"sv).value_or({}).to_byte_string();
        auto* button = new QToolButton(this);
        button->setAutoRaise(true);
        button->setProperty("cgxExtensionId", qstring_from_ak_string(id));
        button->setIconSize({ ACTION_ICON_SIZE, ACTION_ICON_SIZE });
        button->setIcon(action_icon(id, extension, ACTION_ICON_SIZE, devicePixelRatioF(), palette()));
        auto title = qstring_from_ak_string(action->get_string("title"sv).value_or({}));
        auto badge = qstring_from_ak_string(action->get_string("badgeText"sv).value_or({}));
        button->setToolTip(title);
        button->setAccessibleName(badge.isEmpty() ? title : QString("%1 (%2)").arg(title, badge));
        auto enabled = action->get_bool("enabled"sv).value_or(true);
        if (!enabled)
            button->setIcon(QIcon(button->icon().pixmap(ACTION_ICON_SIZE, QIcon::Disabled)));
        QObject::connect(button, &QToolButton::clicked, this, [this, id, button] {
            if (auto* window = browser_window())
                Extensions::open_extension(*window, id, button);
        });
        // The button's own menu: the extension's "action" items, then the browser's.
        button->setContextMenuPolicy(Qt::CustomContextMenu);
        auto options = extension.get_string("optionsUrl"sv);
        QObject::connect(button, &QToolButton::customContextMenuRequested, this, [this, id, button, options](QPoint const& position) {
            auto* menu = new QMenu(button);
            menu->setAttribute(Qt::WA_DeleteOnClose);
            JsonObject target;
            target.set("contexts"sv, JsonArray { Vector<JsonValue> { JsonValue { "action"sv } } });
            if (m_tab && m_tab->is_loaded()) {
                target.set("tabId"sv, tab_id(*m_tab));
                target.set("pageUrl"sv, m_tab->view().url().serialize());
            }
            auto items = ask_core("contextMenuItems"sv, target);
            if (items.is_array()) {
                items.as_array().for_each([&](JsonValue const& entry) {
                    if (!entry.is_object() || entry.as_object().get_string("extensionId"sv).value_or({}).to_byte_string() != id)
                        return;
                    entry.as_object().get_array("items"sv).value_or(JsonArray {}).for_each([&](JsonValue const& item) {
                        if (!item.is_object())
                            return;
                        auto const& object = item.as_object();
                        if (object.get_string("type"sv).value_or({}) == "separator"sv) {
                            menu->addSeparator();
                            return;
                        }
                        auto* action = menu->addAction(qstring_from_ak_string(object.get_string("title"sv).value_or({})));
                        action->setEnabled(object.get_bool("enabled"sv).value_or(true));
                        auto clicked = target;
                        clicked.set("extensionId"sv, MUST(String::from_byte_string(id)));
                        clicked.set("itemId"sv, object.get_string("id"sv).value_or({}));
                        QObject::connect(action, &QAction::triggered, [clicked] { (void)ask_core("contextMenuClicked"sv, clicked); });
                    });
                    menu->addSeparator();
                });
            }
            if (options.has_value()) {
                QObject::connect(menu->addAction("Options"), &QAction::triggered, this, [this, url = *options] {
                    if (auto parsed = URL::Parser::basic_parse(url); parsed.has_value())
                        if (auto* window = browser_window())
                            window->new_tab_from_url(*parsed, Web::HTML::ActivateTab::Yes, BrowserWindow::TabLocation::after_current_tab());
                });
            }
            QObject::connect(menu->addAction("Unpin from toolbar"), &QAction::triggered, [id] {
                JsonObject args;
                args.set("extensionId"sv, MUST(String::from_byte_string(id)));
                args.set("pinned"sv, false);
                (void)ask_core("setActionPinned"sv, args);
            });
            QObject::connect(menu->addAction("Remove from Ladybird…"), &QAction::triggered, [id] { core_remove_extension(id); });
            menu->popup(button->mapToGlobal(position));
        });
        m_buttons_layout->addWidget(button);
        m_action_buttons.append(button);
    });

    if (open_panel() && open_panel()->parent() == this)
        open_panel()->rebuild();
}

// Chrome's shortcut names, in Qt's spelling.
static QKeySequence key_sequence_for(QString shortcut)
{
#    if defined(AK_OS_MACOS)
    shortcut.replace("MacCtrl", "Meta");
    shortcut.replace("Command", "Ctrl");
#    else
    shortcut.replace("MacCtrl", "Ctrl");
    shortcut.replace("Command", "Ctrl");
#    endif
    shortcut.replace("Comma", ",");
    shortcut.replace("Period", ".");
    shortcut.replace("MediaNextTrack", "Media Next");
    shortcut.replace("MediaPrevTrack", "Media Previous");
    shortcut.replace("MediaPlayPause", "Media Play");
    shortcut.replace("MediaStop", "Media Stop");
    return QKeySequence(shortcut);
}

void ExtensionsToolbarArea::rebuild_shortcuts()
{
    auto* window = browser_window();
    if (!window)
        return;
    auto commands = ask_core("commands"sv, JsonObject {});
    auto serialized = commands.serialized().to_byte_string();
    if (serialized == m_last_commands)
        return;
    m_last_commands = serialized;
    for (auto& shortcut : m_shortcuts) {
        if (shortcut)
            shortcut->deleteLater();
    }
    m_shortcuts.clear();
    if (!commands.is_array())
        return;
    commands.as_array().for_each([&](JsonValue const& value) {
        if (!value.is_object())
            return;
        auto const& command = value.as_object();
        auto keys = qstring_from_ak_string(command.get_string("shortcut"sv).value_or({}));
        if (keys.isEmpty())
            return;
        auto id = command.get_string("extensionId"sv).value_or({}).to_byte_string();
        auto name = command.get_string("name"sv).value_or({});
        auto* shortcut = new QShortcut(key_sequence_for(keys), window);
        shortcut->setContext(Qt::WindowShortcut);
        QObject::connect(shortcut, &QShortcut::activated, this, [this, id, name] {
            auto* window = browser_window();
            if (!window)
                return;
            Extensions::report_browser_state();
            JsonObject args;
            args.set("extensionId"sv, MUST(String::from_byte_string(id)));
            args.set("name"sv, name);
            if (auto* tab = window->current_tab(); tab && tab->is_loaded())
                args.set("tabId"sv, tab_id(*tab));
            auto result = ask_core("command"sv, args);
            if (result.is_object()) {
                if (auto popup = result.as_object().get_string("popup"sv); popup.has_value()) {
                    if (auto url = URL::Parser::basic_parse(*popup); url.has_value())
                        show_popup(*window, id, *url, &anchor_for(id));
                }
            }
        });
        m_shortcuts.append(shortcut);
    });
}

// ---- What the extension core asks of the browser ---------------------------------------------------------------------

namespace Extensions {

void ui_changed(StringView, StringView)
{
    // Coalesce: an extension often sets text, colour and icon in a row.
    static bool s_scheduled = false;
    if (s_scheduled)
        return;
    s_scheduled = true;
    QTimer::singleShot(0, [] {
        s_scheduled = false;
        for (auto* window : browser_windows()) {
            if (auto* area = toolbar_area_of(*window))
                area->refresh();
        }
    });
}

static ErrorOr<ByteString> tab_reply(Tab& tab)
{
    report_browser_state();
    return ByteString::formatted("{{\"tabId\":{}}}", tab_id(tab));
}

static Optional<URL::URL> url_argument(JsonObject const& args, StringView key)
{
    auto text = args.get_string(key);
    if (!text.has_value())
        return {};
    return URL::Parser::basic_parse(*text);
}

void host_call(ByteString const& extension_id, StringView function, JsonObject const& args, Function<void(ErrorOr<ByteString>)> reply)
{
    report_browser_state();
    auto ok = [&] {
        report_browser_state();
        reply(ByteString { "null"sv });
    };
    auto fail = [&](StringView message) {
        auto text = ByteString { message };
        reply(Error::from_string_view(text.view()));
    };
    auto tab_arg = [&]() -> Optional<TabPlace> {
        auto id = args.get_integer<i64>("tabId"sv);
        return id.has_value() ? find_tab(*id) : Optional<TabPlace> {};
    };
    auto& application = Application::the();

    if (function == "host.tabs.create"sv) {
        auto url = url_argument(args, "url"sv).value_or(Application::settings().new_tab_page_url());
        auto* window = window_with_id(args.get_integer<i64>("windowId"sv));
        auto active = args.get_bool("active"sv).value_or(true);
        if (!window) {
            auto& created = application.new_window({ url });
            if (auto* tab = created.current_tab())
                return reply(tab_reply(*tab));
            return fail("Could not open a window"sv);
        }
        auto index = args.get_integer<int>("index"sv);
        auto location = index.has_value() && *index >= 0 ? BrowserWindow::TabLocation::at_index(*index) : BrowserWindow::TabLocation::end();
        auto& tab = window->new_tab_from_url(url, active ? Web::HTML::ActivateTab::Yes : Web::HTML::ActivateTab::No, location);
        if (args.get_bool("pinned"sv).value_or(false))
            window->pin_tab(tab);
        return reply(tab_reply(tab));
    }

    if (function == "host.tabs.update"sv) {
        auto place = tab_arg();
        if (!place.has_value())
            return fail("No tab with that id"sv);
        if (auto url = url_argument(args, "url"sv); url.has_value())
            place->tab->navigate(*url);
        if (args.get_bool("active"sv).value_or(false))
            place->window->activate_tab(place->window->tab_index(place->tab));
        if (auto pinned = args.get_bool("pinned"sv); pinned.has_value() && *pinned != place->tab->is_pinned()) {
            if (*pinned)
                place->window->pin_tab(*place->tab);
            else
                place->window->unpin_tab(*place->tab);
        }
        if (auto muted = args.get_bool("muted"sv); muted.has_value()) {
            auto& view = place->tab->view();
            if ((view.page_mute_state() == Web::HTML::MuteState::Muted) != *muted)
                view.toggle_page_mute_state();
        }
        return ok();
    }

    if (function == "host.tabs.remove"sv) {
        auto ids = args.get_array("tabIds"sv).value_or(JsonArray {});
        ids.for_each([&](JsonValue const& id) {
            if (auto place = find_tab(id.get_integer<i64>().value_or(-1)); place.has_value())
                place->window->definitely_close_tab(place->window->tab_index(place->tab));
        });
        return ok();
    }

    if (function == "host.tabs.reload"sv) {
        auto place = tab_arg();
        if (!place.has_value())
            return fail("No tab with that id"sv);
        place->tab->view().reload(args.get_bool("bypassCache"sv).value_or(false) ? WebView::BypassCache::Yes : WebView::BypassCache::No);
        return ok();
    }

    if (function == "host.tabs.navigateHistory"sv) {
        auto place = tab_arg();
        if (!place.has_value())
            return fail("No tab with that id"sv);
        place->tab->view().traverse_the_history_by_delta(args.get_integer<int>("delta"sv).value_or(0));
        return ok();
    }

    if (function == "host.tabs.duplicate"sv) {
        auto place = tab_arg();
        if (!place.has_value())
            return fail("No tab with that id"sv);
        HashTable<i64> before;
        for (int i = 0; i < place->window->tab_count(); ++i)
            before.set(tab_id(*place->window->tab_at(i)));
        place->window->duplicate_tab(*place->tab);
        for (int i = 0; i < place->window->tab_count(); ++i) {
            auto* tab = place->window->tab_at(i);
            if (tab->is_loaded() && !before.contains(tab_id(*tab)))
                return reply(tab_reply(*tab));
        }
        return fail("Could not duplicate the tab"sv);
    }

    if (function == "host.tabs.move"sv) {
        auto* target = window_with_id(args.get_integer<i64>("windowId"sv));
        auto index = args.get_integer<int>("index"sv).value_or(-1);
        auto ids = args.get_array("tabIds"sv).value_or(JsonArray {});
        ids.for_each([&](JsonValue const& id) {
            auto place = find_tab(id.get_integer<i64>().value_or(-1));
            if (!place.has_value() || !target)
                return;
            auto from = place->window->tab_index(place->tab);
            if (place->window == target) {
                auto to = index < 0 || index >= target->tab_count() ? target->tab_count() - 1 : index;
                target->move_tab(from, to);
            } else {
                auto to = index < 0 || index > target->tab_count() ? target->tab_count() : index;
                place->window->move_tab_to_window(from, *target, to);
            }
            if (index >= 0)
                ++index;
        });
        return ok();
    }

    if (function == "host.fontSettings.getFontList"sv) {
        JsonArray fonts;
        for (auto const& family : QFontDatabase::families()) {
            if (QFontDatabase::isPrivateFamily(family))
                continue;
            auto name = ak_string_from_qstring(family);
            JsonObject font;
            font.set("fontId"sv, name);
            font.set("displayName"sv, name);
            fonts.must_append(move(font));
        }
        return reply(fonts.serialized().to_byte_string());
    }

    if (function == "host.tabs.setZoom"sv) {
        auto place = tab_arg();
        if (!place.has_value())
            return fail("No tab with that id"sv);
        place->tab->view().set_zoom(args.get_double_with_precision_loss("zoomFactor"sv).value_or(1.0));
        return ok();
    }

    if (function == "host.tabs.capture"sv) {
        auto place = tab_arg();
        if (!place.has_value())
            return fail("No tab with that id"sv);
        auto pixmap = place->tab->view().grab();
        if (pixmap.isNull())
            return fail("Failed to capture tab: unknown error"sv);
        auto jpeg = args.get_string("format"sv).value_or({}) != "png"sv;
        QByteArray bytes;
        QBuffer buffer(&bytes);
        buffer.open(QIODevice::WriteOnly);
        pixmap.save(&buffer, jpeg ? "JPEG" : "PNG", jpeg ? args.get_integer<int>("quality"sv).value_or(92) : -1);
        auto data_url = ByteString::formatted("{{\"dataUrl\":\"data:image/{};base64,{}\"}}", jpeg ? "jpeg"sv : "png"sv, StringView { bytes.toBase64().constData(), static_cast<size_t>(bytes.toBase64().size()) });
        return reply(data_url);
    }

    if (function == "host.tabs.discard"sv)
        return fail("tabs.discard is not implemented yet in Ladybird"sv);

    if (function == "host.windows.create"sv) {
        Vector<URL::URL> urls;
        args.get_array("urls"sv).value_or(JsonArray {}).for_each([&](JsonValue const& value) {
            if (value.is_string()) {
                if (auto url = URL::Parser::basic_parse(value.as_string()); url.has_value())
                    urls.append(*url);
            }
        });
        WindowConfiguration configuration;
        if (auto x = args.get_integer<int>("left"sv); x.has_value())
            configuration.x = *x;
        if (auto y = args.get_integer<int>("top"sv); y.has_value())
            configuration.y = *y;
        if (auto w = args.get_integer<int>("width"sv); w.has_value())
            configuration.width = *w;
        if (auto h = args.get_integer<int>("height"sv); h.has_value())
            configuration.height = *h;
        if (args.get_string("state"sv).value_or({}) == "maximized"sv)
            configuration.maximized = true;
        auto is_private = args.get_bool("incognito"sv).value_or(false) ? WebView::IsPrivate::Yes : WebView::IsPrivate::No;

        BrowserWindow* window = nullptr;
        if (auto moved = tab_arg(); moved.has_value()) {
            window = &moved->window->detach_tab_to_new_window(moved->window->tab_index(moved->tab), moved->window->geometry().center());
        } else {
            if (urls.is_empty())
                urls.append(Application::settings().new_tab_page_url());
            auto popup = args.get_string("type"sv).value_or({}) == "popup"sv ? BrowserWindow::IsPopupWindow::Yes : BrowserWindow::IsPopupWindow::No;
            window = &application.new_window(urls, configuration, popup, is_private);
        }
        auto state = args.get_string("state"sv).value_or({});
        if (state == "minimized"sv)
            window->showMinimized();
        else if (state == "fullscreen"sv)
            window->showFullScreen();
        if (args.get_bool("focused"sv).value_or(true) && state != "minimized"sv)
            window->bring_to_front();
        report_browser_state();
        return reply(ByteString::formatted("{{\"windowId\":{}}}", window_id(*window)));
    }

    if (function == "host.windows.update"sv) {
        auto* window = window_with_id(args.get_integer<i64>("windowId"sv));
        if (!window)
            return fail("No window with that id"sv);
        auto geometry = window->geometry();
        if (auto x = args.get_integer<int>("left"sv); x.has_value())
            geometry.moveLeft(*x);
        if (auto y = args.get_integer<int>("top"sv); y.has_value())
            geometry.moveTop(*y);
        if (auto w = args.get_integer<int>("width"sv); w.has_value())
            geometry.setWidth(*w);
        if (auto h = args.get_integer<int>("height"sv); h.has_value())
            geometry.setHeight(*h);
        if (geometry != window->geometry())
            window->setGeometry(geometry);
        auto state = args.get_string("state"sv).value_or({});
        if (state == "minimized"sv)
            window->showMinimized();
        else if (state == "maximized"sv)
            window->showMaximized();
        else if (state == "fullscreen"sv)
            window->showFullScreen();
        else if (state == "normal"sv)
            window->showNormal();
        if (args.get_bool("focused"sv) == true)
            window->bring_to_front();
        else if (args.get_bool("focused"sv) == false)
            window->lower();
        if (args.get_bool("drawAttention"sv) == true)
            QApplication::alert(window);
        return ok();
    }

    if (function == "host.windows.remove"sv) {
        auto* window = window_with_id(args.get_integer<i64>("windowId"sv));
        if (!window)
            return fail("No window with that id"sv);
        window->close();
        return ok();
    }

    if (function == "host.action.openPopup"sv) {
        auto* window = window_with_id(args.get_integer<i64>("windowId"sv));
        auto url = url_argument(args, "popupUrl"sv);
        if (!window || !url.has_value())
            return fail("Could not find an active browser window."sv);
        auto* area = toolbar_area_of(*window);
        auto extension_id = core_extension_for_url(url->serialize().bytes_as_string_view()).value_or({});
        show_popup(*window, extension_id, *url, area ? &area->anchor_for(extension_id) : nullptr);
        return ok();
    }

    if (function == "host.history.search"sv) {
        auto text = args.get_string("text"sv).value_or({});
        auto max_results = static_cast<size_t>(args.get_integer<u64>("maxResults"sv).value_or(100));
        auto entries = Application::history_store().list_entries(text, 0, max_results);
        JsonArray out;
        for (auto const& entry : entries) {
            JsonObject item;
            item.set("id"sv, entry.url);
            item.set("url"sv, entry.url);
            item.set("title"sv, entry.title.value_or({}));
            item.set("lastVisitTime"sv, static_cast<double>(entry.last_visited_time.milliseconds_since_epoch()));
            item.set("visitCount"sv, entry.visit_count);
            item.set("typedCount"sv, entry.direct_visit_count);
            out.must_append(move(item));
        }
        return reply(out.serialized().to_byte_string());
    }

    if (function == "host.history.getVisits"sv) {
        auto url = url_argument(args, "url"sv);
        if (!url.has_value())
            return fail("Invalid URL"sv);
        JsonArray out;
        if (auto entry = Application::history_store().entry_for_url(*url); entry.has_value()) {
            // The store keeps only the latest visit of each URL, so that is the one visit reported.
            auto last_visit_ms = entry->last_visited_time.milliseconds_since_epoch();
            JsonObject visit;
            visit.set("id"sv, entry->url);
            visit.set("visitId"sv, String::number(last_visit_ms));
            visit.set("visitTime"sv, static_cast<double>(last_visit_ms));
            visit.set("referringVisitId"sv, "0"sv);
            visit.set("transition"sv, entry->last_direct_visit_time == entry->last_visited_time ? "typed"sv : "link"sv);
            out.must_append(move(visit));
        }
        return reply(out.serialized().to_byte_string());
    }

    if (function == "host.history.addUrl"sv) {
        auto url = url_argument(args, "url"sv);
        if (!url.has_value())
            return fail("Invalid URL"sv);
        Application::history_store().record_visit(*url, args.get_string("title"sv).copy());
        return ok();
    }

    if (function == "host.history.deleteUrl"sv) {
        auto url = url_argument(args, "url"sv);
        if (!url.has_value())
            return fail("Invalid URL"sv);
        Application::history_store().remove_entry_for_url(*url);
        return ok();
    }

    if (function == "host.history.deleteRange"sv) {
        auto start = UnixDateTime::from_milliseconds_since_epoch(static_cast<i64>(args.get_double_with_precision_loss("startTime"sv).value_or(0.0)));
        auto end = UnixDateTime::from_milliseconds_since_epoch(static_cast<i64>(args.get_double_with_precision_loss("endTime"sv).value_or(0.0)));
        auto& history = Application::history_store();

        // The store keeps one entry per URL rather than every visit, so an entry is in the range when its last visit is.
        // Collect first: removing while paging would shift the offsets.
        static constexpr size_t page_size = 500;
        Vector<URL::URL> urls_to_remove;
        for (size_t offset = 0;; offset += page_size) {
            auto entries = history.list_entries({}, offset, page_size);
            for (auto const& entry : entries) {
                if (entry.last_visited_time < start || entry.last_visited_time > end)
                    continue;
                if (auto url = URL::Parser::basic_parse(entry.url); url.has_value())
                    urls_to_remove.append(url.release_value());
            }
            if (entries.size() < page_size)
                break;
        }
        for (auto const& url : urls_to_remove)
            history.remove_entry_for_url(url);
        return ok();
    }

    if (function == "host.history.deleteAll"sv) {
        Application::history_store().remove_entries_accessed_since(UnixDateTime::earliest());
        return ok();
    }

    if (function == "host.topSites.get"sv) {
        auto limit = static_cast<size_t>(args.get_integer<u64>("limit"sv).value_or(12));
        auto entries = Application::history_store().list_entries({}, 0, limit);
        JsonArray out;
        for (auto const& entry : entries) {
            JsonObject item;
            item.set("url"sv, entry.url);
            item.set("title"sv, entry.title.value_or({}));
            out.must_append(move(item));
        }
        return reply(out.serialized().to_byte_string());
    }

    if (function == "host.bookmarks.create"sv) {
        auto title = args.get_string("title"sv).copy();
        if (auto url = url_argument(args, "url"sv); url.has_value())
            Application::bookmark_store().add_bookmark(*url, title, {});
        else
            (void)Application::bookmark_store().add_folder(title);
        return ok();
    }

    if (function == "host.bookmarks.update"sv) {
        auto id = args.get_string("id"sv).value_or({});
        auto title = args.get_string("title"sv).copy();
        if (auto url = url_argument(args, "url"sv); url.has_value())
            Application::bookmark_store().edit_bookmark(id, *url, title);
        else
            Application::bookmark_store().edit_folder(id, title);
        return ok();
    }

    if (function == "host.bookmarks.move"sv) {
        auto id = args.get_string("id"sv).value_or({});
        auto index = static_cast<size_t>(args.get_integer<u64>("index"sv).value_or(0));
        Application::bookmark_store().move_item(id, {}, index);
        return ok();
    }

    if (function == "host.bookmarks.remove"sv) {
        Application::bookmark_store().remove_item(args.get_string("id"sv).value_or({}));
        return ok();
    }

    if (function == "host.search.query"sv) {
        auto text = args.get_string("text"sv).value_or({});
        Optional<WebView::SearchEngine> engine;
        if (auto requested = args.get_string("engine"sv); requested.has_value())
            engine = Application::settings().find_search_engine_by_name(*requested);
        if (!engine.has_value())
            engine = Application::settings().search_engine_settings().engine;
        if (!engine.has_value() && !WebView::builtin_search_engines().is_empty())
            engine = WebView::builtin_search_engines().first();
        if (!engine.has_value())
            return fail("No search engine configured"sv);
        auto search_url = URL::Parser::basic_parse(engine->format_search_query_for_navigation(text));
        if (!search_url.has_value())
            return fail("Invalid search URL"sv);
        auto disposition = args.get_string("disposition"sv).value_or({});
        if (disposition == "NEW_WINDOW"sv) {
            application.new_window(Vector<URL::URL> { *search_url });
        } else if (disposition == "CURRENT_TAB"sv) {
            if (auto* window = application.active_window_if_any(); window && window->current_tab())
                window->current_tab()->view().load(*search_url);
            else
                application.new_window(Vector<URL::URL> { *search_url });
        } else {
            if (auto* window = application.active_window_if_any())
                window->new_tab_from_url(*search_url, Web::HTML::ActivateTab::Yes, BrowserWindow::TabLocation::after_current_tab());
            else
                application.new_window(Vector<URL::URL> { *search_url });
        }
        return ok();
    }

#    if defined(LADYBIRD_HUB_IN_OWN_PROCESS)
    // The Hub answers the primitives that need no windows itself (see extension_host_call_works_without_ui).
    (void)extension_id, (void)args;
    reply(Error::from_string_literal("This primitive needs a browser window, and Ladybird does not handle it yet"));
#    else
    // Fall back to headless/core Application primitives (cookies, browsingData, search.engines, management.uninstall).
    WebView::Application::the().WebView::Application::extension_host_call(extension_id, function, args, move(reply));
#    endif
}

}

}

#endif
