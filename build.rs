use cxx_qt_build::{CxxQtBuilder, QmlModule};
use std::env;
use std::path::PathBuf;

fn get_qt_include_path() -> String {
    // First try environment variable (set by nix)
    if let Ok(path) = env::var("QT_INCLUDE_PATH") {
        if !path.is_empty() {
            return path;
        }
    }

    // Then try qmake
    if let Ok(output) = std::process::Command::new("qmake")
        .args(["-query", "QT_INSTALL_HEADERS"])
        .output()
    {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return path;
        }
    }

    // Then try pkg-config for Qt6Core
    if let Ok(output) = std::process::Command::new("pkg-config")
        .args(["--variable=includedir", "Qt6Core"])
        .output()
    {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return path;
        }
    }

    String::new()
}

fn get_qt_version() -> Option<String> {
    let output = std::process::Command::new("qmake")
        .args(["-query", "QT_VERSION"])
        .output()
        .ok()?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!version.is_empty()).then_some(version)
}

fn get_qt_qml_include_path() -> String {
    if let Ok(path) = env::var("QT_QML_INCLUDE_PATH") {
        if !path.is_empty() {
            return path;
        }
    }

    if let Some(version) = get_qt_version() {
        if let Ok(entries) = std::fs::read_dir("/nix/store") {
            let suffix = format!("-qtdeclarative-{version}");
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                if !name.ends_with(&suffix) {
                    continue;
                }

                let include_path = path.join("include");
                if include_path.join("QtQml").join("QQmlEngine").exists() {
                    return include_path.to_string_lossy().to_string();
                }
            }
        }
    }

    if let Ok(output) = std::process::Command::new("pkg-config")
        .args(["--variable=includedir", "Qt6Qml"])
        .output()
    {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return path;
        }
    }

    String::new()
}

fn get_qt_libexec_path() -> String {
    // First try environment variable (set by nix)
    if let Ok(path) = env::var("QT_LIBEXEC_PATH") {
        if !path.is_empty() {
            return path;
        }
    }

    // Then try qmake
    if let Ok(output) = std::process::Command::new("qmake")
        .args(["-query", "QT_INSTALL_LIBEXECS"])
        .output()
    {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return path;
        }
    }

    String::new()
}

fn main() {
    // Get qtermwidget paths from environment
    let qtermwidget_include = env::var("QTERMWIDGET_INCLUDE_PATH")
        .unwrap_or_else(|_| "/usr/include".to_string());
    let qtermwidget_lib = env::var("QTERMWIDGET_LIB_PATH")
        .unwrap_or_else(|_| "/usr/lib".to_string());

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = PathBuf::from(&out_dir);

    // Get Qt paths
    let qt_include_path = get_qt_include_path();
    let qt_qml_include_path = get_qt_qml_include_path();
    let qt_libexec_path = get_qt_libexec_path();

    // Get the manifest directory (where Cargo.toml is)
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let src_dir = PathBuf::from(&manifest_dir).join("src");
    let header_path = src_dir.join("terminal_widget.h");
    let cpp_path = src_dir.join("terminal_widget.cpp");
    let menu_header_path = src_dir.join("global_menu.h");
    let menu_cpp_path = src_dir.join("global_menu.cpp");

    // Run MOC on the header file
    let moc_output = out_path.join("moc_terminal_widget.cpp");
    let moc_path = if !qt_libexec_path.is_empty() {
        format!("{}/moc", qt_libexec_path)
    } else {
        "moc".to_string()
    };

    let moc_status = std::process::Command::new(&moc_path)
        .arg("-I").arg(&qtermwidget_include)
        .arg("-I").arg(&qt_include_path)
        .arg(&header_path)
        .arg("-o").arg(&moc_output)
        .status()
        .expect("Failed to run moc");

    if !moc_status.success() {
        panic!("moc failed with status: {:?}", moc_status);
    }

    let menu_moc_output = out_path.join("moc_global_menu.cpp");
    let menu_moc_status = std::process::Command::new(&moc_path)
        .arg("-I").arg(&qt_include_path)
        .arg(&menu_header_path)
        .arg("-o").arg(&menu_moc_output)
        .status()
        .expect("Failed to run moc");
    if !menu_moc_status.success() {
        panic!("moc failed with status: {:?}", menu_moc_status);
    }

    // Build the terminal widget C++ code including MOC output
    let mut cc_build = cc::Build::new();
    cc_build
        .cpp(true)
        .file(&cpp_path)
        .file(&moc_output)
        .file(&menu_cpp_path)
        .file(&menu_moc_output)
        .include(&qtermwidget_include)
        .include(&src_dir)
        .include(&out_dir)
        .flag_if_supported("-std=c++17");

    // Add Qt includes
    if !qt_include_path.is_empty() {
        cc_build.include(&qt_include_path);
        cc_build.include(format!("{}/QtCore", qt_include_path));
        cc_build.include(format!("{}/QtGui", qt_include_path));
        cc_build.include(format!("{}/QtWidgets", qt_include_path));
        cc_build.include(format!("{}/QtQml", qt_include_path));
        // The global menu uses the platform theme's menu bar (QPA, private
        // headers: src/global_menu.h).
        if let Some(version) = get_qt_version() {
            cc_build.include(format!("{}/QtCore/{}", qt_include_path, version));
            cc_build.include(format!("{}/QtCore/{}/QtCore", qt_include_path, version));
            cc_build.include(format!("{}/QtGui/{}", qt_include_path, version));
            cc_build.include(format!("{}/QtGui/{}/QtGui", qt_include_path, version));
        }
    }
    if !qt_qml_include_path.is_empty() {
        cc_build.include(&qt_qml_include_path);
        cc_build.include(format!("{}/QtQml", qt_qml_include_path));
        cc_build.include(format!("{}/QtQmlIntegration", qt_qml_include_path));
        // QQuickWindow::setTextRenderType (native text rendering).
        cc_build.include(format!("{}/QtQuick", qt_qml_include_path));
    }

    cc_build.compile("terminal_widget");

    // Link to qtermwidget
    println!("cargo:rustc-link-search=native={}", qtermwidget_lib);
    println!("cargo:rustc-link-lib=qtermwidget6");

    // Tell cargo to rerun if our source files change
    println!("cargo:rerun-if-changed={}", header_path.display());
    println!("cargo:rerun-if-changed={}", cpp_path.display());
    println!("cargo:rerun-if-changed={}", menu_header_path.display());
    println!("cargo:rerun-if-changed={}", menu_cpp_path.display());

    // Build the main cxx-qt parts
    CxxQtBuilder::new()
        .qt_module("Network")
        .qt_module("Widgets")
        .qml_module(QmlModule {
            uri: "NixosUpdateChecker",
            rust_files: &["src/tray.rs"],
            qml_files: &["qml/main.qml", "qml/AppMenu.qml", "qml/MacComboBox.qml", "qml/MacComboItem.qml", "qml/MacComboPopup.qml", "qml/MacMenuBackground.qml", "qml/MacMenuIcon.qml", "qml/MacMenuItem.qml", "qml/MacMenu.qml", "qml/MacMenuButton.qml", "qml/MacMenuSeparator.qml", "qml/MacMenuStyle.qml", "qml/ChromeButtons.qml", "qml/ChromeResizeEdges.qml", "qml/ChromeTitleArea.qml"],
            ..Default::default()
        })
        .qrc("resources/resources.qrc")
        .build();
}
