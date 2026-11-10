#pragma once
#include <QQmlApplicationEngine>
#include <QQuickWindow>
#include <QQuickItem>
#include <QTimer>
#include <QFile>
#include <QSaveFile>
#include <QDir>
#include <QCoreApplication>
#include <QEvent>
#include <QUrl>
#include <QJsonDocument>
#include <QJsonObject>
#include <QFileInfo>
#include <QJsonArray>
#include <algorithm>
#include <functional>
#include "artwork_provider.h"
#ifndef QT_GUI_LIB
#define QT_GUI_LIB
#endif
#include <QtTest/qtestkeyboard.h>
#include <QtTest/qtestmouse.h>

// Visible text that spills out of its own box, or that a non-scrolling clip or
// the window edge cuts off. Scrolling content is exempt only along an axis the
// enclosing Flickable can actually scroll. Elided text is listed separately.
inline QJsonObject rustris_layout_audit(QQuickWindow* window) {
    struct Region { QRectF clip; bool scrollX; bool scrollY; };
    QJsonArray issues;
    QJsonArray elided;
    int texts = 0;
    const auto json = [](const QRectF& r) {
        return QJsonArray{qRound(r.x()), qRound(r.y()), qRound(r.width()), qRound(r.height())};
    };
    std::function<void(QQuickItem*, Region)> visit = [&](QQuickItem* item, Region region) {
        if (!item->isVisible() || item->opacity() <= 0.0) return;
        const QRectF box = item->mapRectToScene(QRectF(0, 0, item->width(), item->height()));
        // A read-only TextInput, or the display of a non-editable ComboBox,
        // cannot be scrolled by the user, so hidden content there is overflow.
        const auto parent = item->parentItem();
        const bool readOnlyInput = item->inherits("QQuickTextInput") && (item->property("readOnly").toBool() ||
            (parent && parent->inherits("QQuickComboBox") && !parent->property("editable").toBool()));
        const bool isText = item->inherits("QQuickText") || item->inherits("QQuickTextEdit") || readOnlyInput;
        const QString text = isText ? item->property("text").toString() : QString();
        if (isText && !text.trimmed().isEmpty()) {
            ++texts;
            const qreal contentWidth = item->property("contentWidth").toReal();
            const qreal contentHeight = item->property("contentHeight").toReal();
            const bool truncated = item->property("truncated").toBool();
            const bool elides = item->property("elide").toInt() != 0;
            const QRectF drawn = item->mapRectToScene(QRectF(0, 0,
                std::max<qreal>(item->width(), contentWidth), std::max<qreal>(item->height(), contentHeight)));
            QJsonObject entry{{"text", text.left(80)}, {"class", item->metaObject()->className()},
                {"box", json(box)}, {"content", QJsonArray{qRound(contentWidth), qRound(contentHeight)}}};
            QStringList kinds;
            if (!item->inherits("QQuickTextEdit") && !truncated &&
                ((!elides && contentWidth > item->width() + 1) || contentHeight > item->height() + 1))
                kinds << "overflow";
            const bool outsideX = drawn.left() < region.clip.left() - 1 || drawn.right() > region.clip.right() + 1;
            const bool outsideY = drawn.top() < region.clip.top() - 1 || drawn.bottom() > region.clip.bottom() + 1;
            const bool fullyHidden = !drawn.intersects(region.clip);
            if (!fullyHidden && ((outsideX && !region.scrollX) || (outsideY && !region.scrollY))) {
                kinds << "clipped";
                entry["clip"] = json(region.clip);
            }
            if (fullyHidden && !region.scrollX && !region.scrollY) {
                kinds << "hidden";
                entry["clip"] = json(region.clip);
            }
            if (!kinds.isEmpty()) {
                entry["kinds"] = QJsonArray::fromStringList(kinds);
                issues.append(entry);
            }
            if (truncated) elided.append(entry);
        }
        Region inner = region;
        if (item->clip()) inner.clip = region.clip.intersected(box);
        if (item->inherits("QQuickFlickable")) {
            inner.scrollX = region.scrollX || item->property("contentWidth").toReal() > item->width() + 1;
            inner.scrollY = region.scrollY || item->property("contentHeight").toReal() > item->height() + 1;
        }
        for (auto child : item->childItems()) visit(child, inner);
    };
    visit(window->contentItem(), Region{QRectF(0, 0, window->width(), window->height()), false, false});
    // An open dialog should show its title once; a default header on top of a
    // heading in the content repeats it.
    std::function<int(QQuickItem*, const QString&)> count = [&](QQuickItem* item, const QString& title) {
        if (!item || !item->isVisible() || item->opacity() <= 0.0) return 0;
        int found = item->inherits("QQuickText") && item->property("text").toString() == title ? 1 : 0;
        for (auto child : item->childItems()) found += count(child, title);
        return found;
    };
    for (auto popup : window->findChildren<QObject*>()) {
        if (!popup->inherits("QQuickDialog") || !popup->property("opened").toBool()) continue;
        const auto title = popup->property("title").toString();
        if (title.isEmpty()) continue;
        const int shown = count(popup->property("header").value<QQuickItem*>(), title) +
            count(popup->property("contentItem").value<QQuickItem*>(), title);
        if (shown > 1)
            issues.append(QJsonObject{{"text", title.left(80)}, {"class", popup->metaObject()->className()},
                {"kinds", QJsonArray{"duplicate-title"}}, {"count", shown}});
    }
    return QJsonObject{{"texts", texts}, {"issues", issues}, {"elided", elided}};
}

// A deterministic QtTest transport, available only with the test-support feature.
// Commands use Qt's event APIs. The artwork probe only reads a file inside the test directory.
inline void rustris_install_test_driver(QQmlApplicationEngine& engine) {
    const auto directory = qEnvironmentVariable("RUSTRIS_TEST_DIRECTORY");
    if (directory.isEmpty()) return;
    auto timer = new QTimer(&engine);
    QObject::connect(timer, &QTimer::timeout, &engine, [&engine, directory, last = QString()]() mutable {
        QFile request(directory + "/qt-request.json");
        if (!request.open(QIODevice::ReadOnly)) return;
        const auto command = QJsonDocument::fromJson(request.readAll()).object();
        const auto id = command["id"].toString();
        if (id.isEmpty() || id == last) return;
        last = id;
        auto window = qobject_cast<QQuickWindow*>(engine.rootObjects().value(0));
        if (!window) return;
        QString error;
        QImage decoded;
        bool probed = false;
        bool cancelProbed = false;
        bool cancelFinished = false;
        bool cancelHasTexture = false;
        QJsonObject layout;
        bool audited = false;
        const auto action = command["action"].toString();
        if (action == "key") {
            const auto sequence = QKeySequence::fromString(command["sequence"].toString());
            auto target = QGuiApplication::focusWindow();
            if (!target) target = window;
            for (int i = 0; i < sequence.count(); ++i) {
                const auto key = sequence[i];
                QTest::keyClick(target, key.key(), key.keyboardModifiers());
            }
        } else if (action == "type-ascii") {
            const auto value = command["text"].toString();
            if (value.size() > 1024 || std::any_of(value.begin(), value.end(), [](QChar c) {
                return c.unicode() < 32 || c.unicode() > 126;
            })) {
                error = "Expected bounded printable ASCII";
            } else {
                auto target = QGuiApplication::focusWindow();
                if (!target) target = window;
                for (const auto c : value) QTest::keyClick(target, c.toLatin1());
            }
        } else if (action == "double-click") {
            auto target = QGuiApplication::focusWindow();
            if (!target) target = window;
            QTest::mouseDClick(target, Qt::LeftButton, Qt::NoModifier,
                QPoint(command["x"].toInt(), command["y"].toInt()));
        } else if (action == "resize") {
            window->resize(command["width"].toInt(), command["height"].toInt());
        } else if (action == "layout-audit") {
            layout = rustris_layout_audit(window);
            audited = true;
        } else if (action == "screenshot") {
            if (!window->grabWindow().save(directory + "/screenshot.png")) error = "Cannot capture the window";
        } else if (action == "recreate") {
            // Deliberately queue work, then destroy the result's Qt owner.
            auto backend = window->findChild<QObject*>("libraryBackend");
            if (!backend || !QMetaObject::invokeMethod(backend, "load")) error = "Cannot queue view work";
            auto sessions = window->findChild<QObject*>("sessionBackend");
            if (!sessions || !QMetaObject::invokeMethod(sessions, "refresh")) error = "Cannot queue session view work";
            delete window;
            engine.load(QUrl("qrc:/rustris/App.qml"));
            window = qobject_cast<QQuickWindow*>(engine.rootObjects().constLast());
        } else if (action == "close") {
            QTimer::singleShot(100, window, &QWindow::close);
        } else if (action == "release-artwork-gate") {
            auto& gate = rustris_artwork_test_gate();
            if (!gate.blocked.load()) error = "Artwork gate is not blocked";
            else gate.resume.release();
        } else if (action == "decode-artwork" || action == "cancel-artwork" || action == "cancel-decoded-artwork" || action == "arm-artwork-gate") {
            const auto relative = command["path"].toString();
            const auto full = QFileInfo(QDir(directory).filePath(relative)).canonicalFilePath();
            const auto root = QDir(directory).canonicalPath();
            if (relative.isEmpty() || relative.contains(QLatin1String("..")) || full.isEmpty() ||
                !full.startsWith(root + QLatin1Char('/')))
                error = "Artwork probe path is outside the test directory";
            else if (action == "decode-artwork") {
                decoded = rustris_read_artwork(full.toUtf8(), QSize(command["width"].toInt(), command["height"].toInt()));
                probed = true;
            } else if (action == "arm-artwork-gate") {
                if (!rustris_artwork_test_gate().arm(full.toUtf8().toHex())) error = "Artwork gate is already active";
            } else {
                RustrisArtworkResponse response(QString::fromLatin1(full.toUtf8().toHex()), QSize(144, 180));
                QObject::connect(&response, &QQuickImageResponse::finished, [&cancelFinished] { cancelFinished = true; });
                if (action == "cancel-artwork") response.cancel();
                response.run();
                if (action == "cancel-decoded-artwork") response.cancel();
                QCoreApplication::sendPostedEvents(&response, QEvent::MetaCall);
                auto* texture = response.textureFactory();
                cancelHasTexture = texture != nullptr;
                delete texture;
                cancelProbed = true;
            }
        } else if (action != "state") error = "Unknown test action";
        if (!window) return;
        QJsonObject answer{{"id", id}, {"error", error}, {"dpr", window->devicePixelRatio()},
            {"width", window->width()}, {"height", window->height()}};
        auto& gate = rustris_artwork_test_gate();
        answer["artworkGateBlocked"] = gate.blocked.load();
        answer["artworkGateCompleted"] = gate.completed.load();
        answer["artworkGateCancelled"] = gate.cancelled.load();
        answer["artworkGateTimedOut"] = gate.timedOut.load();
        int literalLabels = 0;
        int richUserLabels = 0;
        for (auto object : window->findChildren<QObject*>()) {
            if (object->property("text").toString().contains("<b>literal</b>") && object->property("textFormat").isValid()) {
                ++literalLabels;
                if (object->property("textFormat").toInt() != 0) ++richUserLabels;
            }
        }
        answer["literalLabels"] = literalLabels;
        answer["richUserLabels"] = richUserLabels;
        if (probed) {
            answer["decodedWidth"] = decoded.width();
            answer["decodedHeight"] = decoded.height();
            answer["decodedBytes"] = static_cast<int>(decoded.sizeInBytes());
        }
        if (audited) answer["layout"] = layout;
        if (cancelProbed) {
            answer["cancelFinished"] = cancelFinished;
            answer["cancelHasTexture"] = cancelHasTexture;
        }
        if (auto artwork = window->findChild<QObject*>("selectedArtwork")) {
            const auto source = artwork->property("source").toUrl();
            answer["artworkSource"] = source.scheme() == "image" && source.host() == "rustris-art"
                ? QString::fromUtf8(QByteArray::fromHex(source.path().mid(1).toLatin1()))
                : source.toLocalFile();
            answer["artworkStatus"] = artwork->property("status").toInt();
            if (auto item = qobject_cast<QQuickItem*>(artwork)) {
                const auto center = item->mapToScene(QPointF(item->width() / 2, item->height() / 2));
                answer["artworkCenterX"] = center.x();
                answer["artworkCenterY"] = center.y();
            }
        }
        if (auto focus = window->activeFocusItem()) {
            answer["focusClass"] = QString(focus->metaObject()->className());
            answer["focusName"] = focus->objectName();
        }
        QSaveFile response(directory + "/qt-response.json");
        if (response.open(QIODevice::WriteOnly)) {
            response.write(QJsonDocument(answer).toJson());
            response.commit();
        }
    });
    timer->start(10);
}
