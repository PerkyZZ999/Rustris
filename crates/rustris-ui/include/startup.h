#pragma once
#include <QQmlApplicationEngine>
#include <QImageReader>
inline void rustris_limit_image_allocations() {
    QImageReader::setAllocationLimit(64);
}
inline bool rustris_engine_loaded(const QQmlApplicationEngine& engine) {
    return !engine.rootObjects().isEmpty();
}
