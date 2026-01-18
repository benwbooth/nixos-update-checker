mod config;
mod flake_checker;
mod tray;
mod update_runner;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    // Initialize Qt application
    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    // Load the QML UI
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/NixosUpdateChecker/qml/main.qml"));
    }

    // Run the Qt event loop
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
