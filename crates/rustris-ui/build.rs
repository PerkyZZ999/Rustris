fn main() {
    println!("cargo:rerun-if-changed=resources.qrc");
    for relative in [
        "../../apps/desktop/qml/App.qml",
        "../../apps/desktop/qml/components",
        // Explicit rerun paths disable Cargo's package-wide default, so the
        // C++ headers compiled into the bridge must be listed as well.
        "include",
    ] {
        println!("cargo:rerun-if-changed={relative}");
    }
    let mut builder = cxx_qt_build::CxxQtBuilder::new_qml_module(
        cxx_qt_build::QmlModule::new("Rustris").depend("QtQml.Models"),
    )
    .file("src/backend.rs")
    .file("src/sessions.rs")
    .file("src/engine.rs")
    .include_dir("include")
    .qrc("resources.qrc")
    .qt_module("Quick");
    if std::env::var_os("CARGO_FEATURE_TEST_SUPPORT").is_some() {
        builder = builder.file("src/test_support.rs").qt_module("Test");
        // SAFETY: This only adds a compile-time definition for the test build.
        builder = unsafe {
            builder.cc_builder(|cc| {
                cc.define("RUSTRIS_ARTWORK_TEST_GATE", None);
            })
        };
    }
    builder.build();
}
