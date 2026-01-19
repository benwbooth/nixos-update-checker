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

## Known Qt Bugs

- **Do NOT add `icon.source` to ApplicationWindow** - it breaks Qt.labs.platform.SystemTrayIcon (causes tray icon to not appear)
- SystemTrayIcon initial `icon.source` must be a static qrc path, not a function call like `checker.get_icon_path()` - dynamic updates work via signal handlers
