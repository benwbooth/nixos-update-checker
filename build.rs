use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new()
        .qt_module("Network")
        .qml_module(QmlModule {
            uri: "NixosUpdateChecker",
            rust_files: &["src/tray.rs"],
            qml_files: &["qml/main.qml"],
            ..Default::default()
        })
        .qrc("resources/resources.qrc")
        .build();
}
