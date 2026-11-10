//! Compiled only into explicit test builds; never part of the default executable.
#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qqmlapplicationengine.h");
        type QQmlApplicationEngine = cxx_qt_lib::QQmlApplicationEngine;
        include!("test_driver.h");
        fn rustris_install_test_driver(engine: Pin<&mut QQmlApplicationEngine>);
    }
}
pub fn install(engine: std::pin::Pin<&mut cxx_qt_lib::QQmlApplicationEngine>) {
    ffi::rustris_install_test_driver(engine);
}
