use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};
use std::process::ExitCode;

fn run() -> Result<u8, Box<dyn std::error::Error>> {
    cxx_qt::init_crate!(rustris_ui);
    cxx_qt::init_crate!(cxx_qt_lib);
    rustris_ui::limit_image_allocations();
    let worker = rustris_ui::initialize()?;
    let mut app = QGuiApplication::new();
    app.pin_mut()
        .set_application_name(&QString::from("Rustris"));
    let mut engine = QQmlApplicationEngine::new();
    rustris_ui::install_artwork_provider(engine.pin_mut());
    engine.pin_mut().load(&QUrl::from("qrc:/rustris/App.qml"));
    if !rustris_ui::engine_loaded(&engine) {
        drop(engine);
        drop(app);
        worker.shutdown()?;
        return Err(
            "The Rustris interface could not be loaded; see the Qt diagnostic above.".into(),
        );
    }
    #[cfg(feature = "test-support")]
    rustris_ui::install_test_driver(engine.pin_mut());
    let code = app.pin_mut().exec();
    drop(engine);
    drop(app);
    worker.shutdown()?;
    Ok(u8::try_from(code).unwrap_or(1))
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            tracing::error!(%error, "Rustris could not continue");
            ExitCode::FAILURE
        }
    }
}
