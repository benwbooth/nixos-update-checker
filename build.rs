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
    // Emit build info env vars
    let git_hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_GIT_HASH={}", git_hash);

    let build_time = std::process::Command::new("date")
        .arg("+%Y-%m-%d %H:%M:%S %Z")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", build_time);

    // Get qtermwidget paths from environment
    let qtermwidget_include = env::var("QTERMWIDGET_INCLUDE_PATH")
        .unwrap_or_else(|_| "/usr/include".to_string());
    let qtermwidget_lib = env::var("QTERMWIDGET_LIB_PATH")
        .unwrap_or_else(|_| "/usr/lib".to_string());

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = PathBuf::from(&out_dir);

    // Get Qt paths
    let qt_include_path = get_qt_include_path();
    let qt_libexec_path = get_qt_libexec_path();

    // Get the manifest directory (where Cargo.toml is)
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let src_dir = PathBuf::from(&manifest_dir).join("src");
    let header_path = src_dir.join("terminal_widget.h");
    let cpp_path = src_dir.join("terminal_widget.cpp");

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

    // Build the terminal widget C++ code including MOC output
    let mut cc_build = cc::Build::new();
    cc_build
        .cpp(true)
        .file(&cpp_path)
        .file(&moc_output)
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
    }

    cc_build.compile("terminal_widget");

    // Link to qtermwidget
    println!("cargo:rustc-link-search=native={}", qtermwidget_lib);
    println!("cargo:rustc-link-lib=qtermwidget6");

    // Tell cargo to rerun if our source files change
    println!("cargo:rerun-if-changed={}", header_path.display());
    println!("cargo:rerun-if-changed={}", cpp_path.display());

    // Build the main cxx-qt parts
    CxxQtBuilder::new()
        .qt_module("Network")
        .qt_module("Widgets")
        .qml_module(QmlModule {
            uri: "NixosUpdateChecker",
            rust_files: &["src/tray.rs"],
            qml_files: &["qml/main.qml"],
            ..Default::default()
        })
        .qrc("resources/resources.qrc")
        .build();
}
