#pragma once

#include <QBuffer>
#include <QFile>
#include <QImageReader>
#include <QMetaObject>
#include <QQmlApplicationEngine>
#include <QQuickAsyncImageProvider>
#include <QRunnable>
#include <QThreadPool>
#include <algorithm>
#include <atomic>
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>
#ifdef RUSTRIS_ARTWORK_TEST_GATE
#include <QMutex>
#include <QMutexLocker>
#include <QSemaphore>

struct RustrisArtworkTestGate {
    QMutex mutex;
    QByteArray target;
    bool armed = false;
    QSemaphore resume;
    std::atomic_bool active{false};
    std::atomic_bool blocked{false};
    std::atomic_bool completed{false};
    std::atomic_bool cancelled{false};
    std::atomic_bool timedOut{false};

    bool arm(const QByteArray& id) {
        QMutexLocker lock(&mutex);
        if (armed || active.load()) return false;
        while (resume.tryAcquire()) {}
        target = id;
        completed.store(false);
        cancelled.store(false);
        timedOut.store(false);
        armed = true;
        return true;
    }

    void pause(const QByteArray& id, const std::atomic_bool& responseCancelled) {
        {
            QMutexLocker lock(&mutex);
            if (!armed || target != id) return;
            armed = false;
            active.store(true);
        }
        blocked.store(true);
        const bool released = resume.tryAcquire(1, 30000);
        cancelled.store(responseCancelled.load(std::memory_order_acquire));
        timedOut.store(!released);
        blocked.store(false);
        completed.store(true);
        active.store(false);
    }
};

inline RustrisArtworkTestGate& rustris_artwork_test_gate() {
    static auto* gate = new RustrisArtworkTestGate;
    return *gate;
}
#endif

// One decode at a time. Each request reopens the path and keeps that inode, so a
// later replacement cannot be read as this image or block the next selection.
inline QThreadPool* rustris_artwork_pool() {
    static auto* pool = [] {
        auto* created = new QThreadPool;
        created->setMaxThreadCount(1);
        created->setExpiryTimeout(-1);
        return created;
    }();
    return pool;
}

inline QImage rustris_read_artwork(const QByteArray& path, const QSize& requestedSize) {
    constexpr qsizetype maxInputBytes = 32 * 1024 * 1024;
    const int identity = ::open(path.constData(), O_PATH | O_CLOEXEC);
    if (identity < 0) return {};
    struct stat observed {};
    if (::fstat(identity, &observed) != 0 || !S_ISREG(observed.st_mode) ||
        observed.st_size < 0 || observed.st_size > maxInputBytes) {
        ::close(identity);
        return {};
    }
    const QByteArray descriptorPath = QByteArray("/proc/self/fd/") + QByteArray::number(identity);
    const int descriptor = ::open(descriptorPath.constData(), O_RDONLY | O_CLOEXEC);
    ::close(identity);
    if (descriptor < 0) return {};
    struct stat opened {};
    if (::fstat(descriptor, &opened) != 0 || !S_ISREG(opened.st_mode) ||
        opened.st_dev != observed.st_dev || opened.st_ino != observed.st_ino ||
        opened.st_size < 0 || opened.st_size > maxInputBytes) {
        ::close(descriptor);
        return {};
    }
    QFile file;
    if (!file.open(descriptor, QIODevice::ReadOnly, QFileDevice::AutoCloseHandle)) {
        ::close(descriptor);
        return {};
    }
    QByteArray input;
    input.reserve(static_cast<qsizetype>(opened.st_size));
    char block[64 * 1024];
    for (;;) {
        const qint64 count = file.read(block, sizeof(block));
        if (count < 0 || input.size() > maxInputBytes - count) return {};
        if (count == 0) break;
        input.append(block, static_cast<qsizetype>(count));
    }
    QBuffer buffer(&input);
    if (!buffer.open(QIODevice::ReadOnly)) return {};
    QImageReader reader(&buffer);
    reader.setAutoTransform(true);
    reader.setDecideFormatFromContent(true);
    const QSize original = reader.size();
    const int limitMb = QImageReader::allocationLimit();
    bool withinLimit = limitMb <= 0 || !original.isValid() || original.width() <= 0 || original.height() <= 0;
    if (!withinLimit) {
        const qint64 limitBytes = qint64(limitMb) * 1024 * 1024;
        withinLimit = qint64(original.width()) <= limitBytes / 4 / qMax(original.height(), 1);
    }
    // One decode runs at a time. Its output stays inside this edge, so the
    // retained decoded image cannot grow with the source or the request.
    constexpr int maxEdge = 256;
    QSize target(maxEdge, maxEdge);
    if (requestedSize.isValid() && requestedSize.width() > 0 && requestedSize.height() > 0)
        target = requestedSize.boundedTo(QSize(maxEdge, maxEdge));
    if (withinLimit && original.isValid())
        reader.setScaledSize(original.scaled(target, Qt::KeepAspectRatio));
    const QImage image = reader.read();
    if (image.isNull() || image.width() > maxEdge || image.height() > maxEdge ||
        image.sizeInBytes() > qsizetype(maxEdge) * maxEdge * 4)
        return {};
    return image;
}

class RustrisArtworkResponse final : public QQuickImageResponse, public QRunnable {
public:
    RustrisArtworkResponse(QString id, QSize requestedSize)
        : id(std::move(id)), requestedSize(requestedSize) {
        setAutoDelete(false);
    }

    void cancel() override { cancelled.store(true, std::memory_order_release); }

    QQuickTextureFactory* textureFactory() const override {
        return image.isNull() || cancelled.load(std::memory_order_acquire)
            ? nullptr
            : QQuickTextureFactory::textureFactoryForImage(image);
    }

    QString errorString() const override { return error; }

    void run() override {
        if (!cancelled.load(std::memory_order_acquire)) {
            constexpr qsizetype maxPathBytes = 64 * 1024;
            const QByteArray encoded = id.toLatin1();
            if (!encoded.isEmpty() && encoded.size() <= maxPathBytes * 2 && encoded.size() % 2 == 0 &&
                std::all_of(encoded.cbegin(), encoded.cend(), [](char c) {
                    return (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f');
                })) {
                const QByteArray path = QByteArray::fromHex(encoded);
                if (!path.isEmpty() && path.front() == '/' && !path.contains('\0') &&
                    !cancelled.load(std::memory_order_acquire))
                    image = rustris_read_artwork(path, requestedSize);
            }
        }
#ifdef RUSTRIS_ARTWORK_TEST_GATE
        if (!image.isNull()) rustris_artwork_test_gate().pause(id.toLatin1(), cancelled);
#endif
        if (image.isNull()) error = QStringLiteral("The artwork file could not be read.");
        if (!published.test_and_set(std::memory_order_acq_rel))
            QMetaObject::invokeMethod(this, "finished", Qt::QueuedConnection);
    }

private:
    QString id;
    QSize requestedSize;
    QImage image;
    QString error;
    std::atomic_bool cancelled{false};
    std::atomic_flag published = ATOMIC_FLAG_INIT;
};

class RustrisArtworkProvider final : public QQuickAsyncImageProvider {
public:
    QQuickImageResponse* requestImageResponse(const QString& id, const QSize& requestedSize) override {
        auto* response = new RustrisArtworkResponse(id, requestedSize);
        rustris_artwork_pool()->start(response);
        return response;
    }
};

inline void rustris_install_artwork_provider(QQmlApplicationEngine& engine) {
    engine.addImageProvider(QStringLiteral("rustris-art"), new RustrisArtworkProvider());
}
