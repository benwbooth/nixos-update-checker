mod config;
mod flake_checker;
mod tray;

use cxx_qt_lib::{QQmlApplicationEngine, QUrl};
use cxx_qt_lib_extras::QApplication;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// Get the path to the lock file
fn get_lock_file_path() -> PathBuf {
    directories::ProjectDirs::from("", "", "nixos-update-checker")
        .map(|dirs| dirs.runtime_dir().unwrap_or(dirs.cache_dir()).to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("nixos-update-checker.lock")
}

/// Try to acquire a lock file. Returns the file handle if successful.
fn try_acquire_lock() -> Option<File> {
    let lock_path = get_lock_file_path();

    // Create parent directory if needed
    if let Some(parent) = lock_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Try to open with exclusive access
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&lock_path)
        .ok()?;

    // Try to get an exclusive lock (non-blocking)
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = file.as_raw_fd();
        let result = unsafe {
            libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB)
        };
        if result != 0 {
            return None;
        }
    }

    // Write our PID to the lock file
    let mut file = file;
    let _ = writeln!(file, "{}", std::process::id());
    let _ = file.flush();

    Some(file)
}

fn main() {
    // Ensure only one instance is running
    let _lock = match try_acquire_lock() {
        Some(lock) => lock,
        None => {
            eprintln!("Another instance of nixos-update-checker is already running.");
            std::process::exit(1);
        }
    };

    // Initialize Qt application (QApplication is required for Qt.labs.platform.SystemTrayIcon)
    let mut app = QApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    // Load the QML UI
    if let Some(engine) = engine.as_mut() {
        // Try loading the QML file
        let qml_path = "qrc:/qt/qml/NixosUpdateChecker/qml/main.qml";
        eprintln!("Loading QML from: {}", qml_path);
        engine.load(&QUrl::from(qml_path));
        eprintln!("QML load() returned");
    }

    eprintln!("Starting Qt event loop...");

    // Run the Qt event loop
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
