#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qqmlapplicationengine.h");
        type QQmlApplicationEngine = cxx_qt_lib::QQmlApplicationEngine;
        include!("startup.h");
        include!("artwork_provider.h");
        fn rustris_engine_loaded(engine: &QQmlApplicationEngine) -> bool;
        fn rustris_limit_image_allocations();
        fn rustris_install_artwork_provider(engine: Pin<&mut QQmlApplicationEngine>);
    }
}
pub fn limit_image_allocations() {
    ffi::rustris_limit_image_allocations();
}
pub fn loaded(engine: &cxx_qt_lib::QQmlApplicationEngine) -> bool {
    ffi::rustris_engine_loaded(engine)
}
pub fn install_artwork_provider(engine: std::pin::Pin<&mut cxx_qt_lib::QQmlApplicationEngine>) {
    ffi::rustris_install_artwork_provider(engine);
}
