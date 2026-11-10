# NixOS Update Checker - Development Notes

## Building and Running

Always run `cargo build` first before `cargo run` to avoid issues with interrupted builds:

```bash
nix develop --command cargo build
nix develop --command cargo run
```

## Project Structure

- `src/` - Rust source code
  - `main.rs` - Application entry point
  - `tray.rs` - System tray and update checker logic (cxx-qt bridge)
  - `config.rs` - Configuration management
  - `flake_checker.rs` - Flake update detection
- `qml/` - QML UI files
- `resources/` - Icons and desktop file

## Workflow

- **Always commit and push after implementing changes** - don't leave work uncommitted
- Test with `nix build` before committing to ensure the package builds correctly

## Notes

- Must run inside `nix develop` shell for Qt dependencies
- Uses cxx-qt for Rust/Qt6 bindings
- System tray requires Qt.labs.platform (QApplication, not QGuiApplication)
- Do NOT run `cargo clean` unnecessarily - it wastes time rebuilding

## System Theme (KDE)

The app draws with the system KDE theme, which takes two things:

- `resources/qtquickcontrols2.conf` (in the qrc, so it applies to every way of
  starting the app) selects the `org.kde.desktop` Quick Controls style.
- The dev shell puts this flake's Qt/KDE plugins and QML modules first on
  `QT_PLUGIN_PATH` / `QML_IMPORT_PATH` (see `qtRuntimePaths` in `flake.nix`).
  The packaged build gets the same from `wrapQtAppsHook`.

Both are needed: the Plasma session exports its own plugin paths, but those
plugins are built against a newer Qt than this flake's, so Qt refuses to load
them ("uses incompatible Qt library"). With the style selected and no loadable
plugin, QML fails to import `QtQuick.Controls` at all and no window appears.

## Known Qt Bugs

- **Do NOT add `icon.source` to ApplicationWindow** - it breaks Qt.labs.platform.SystemTrayIcon (causes tray icon to not appear)
- SystemTrayIcon initial `icon.source` must be a static qrc path, not a function call like `checker.get_icon_path()` - dynamic updates work via signal handlers
