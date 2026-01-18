mod config;
mod flake_checker;
mod tray;
mod update_runner;

use cxx_qt_lib::{QQmlApplicationEngine, QUrl};
use cxx_qt_lib_extras::QApplication;

fn main() {
    // Initialize Qt application (QApplication is required for Qt.labs.platform.SystemTrayIcon)
    let mut app = QApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    // Load the QML UI
    if let Some(engine) = engine.as_mut() {
        // Try loading the QML file
        let qml_path = "qrc:/qt/qml/NixosUpdateChecker/qml/main.qml";
        eprintln!("Loading QML from: {}", qml_path);
        engine.load(&QUrl::from(qml_path));
    }

    eprintln!("Starting Qt event loop...");

    // Run the Qt event loop
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
