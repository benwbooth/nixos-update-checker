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

## Menus and the global menu bar

The menus are defined once, as `menuModel` in `qml/main.qml`, and shown in
two places: the hamburger button in the window and the desktop's global menu
bar (hearth's top bar, Plasma's Global Menu). Qt Quick's `MenuBar` only goes
native on macOS (`QQuickMenuBarPrivate::useNativeMenuBar()` returns false
elsewhere), so `src/global_menu.{h,cpp}` builds the platform theme's menu bar
directly, as `QMenuBar` does: menus come from `QPlatformMenuBar::createMenu()`
(the theme's `createPlatformMenu()` returns none under KDE). Under the KDE
platform theme that is a dbusmenu whose address goes on the window through
the KDE appmenu protocol; arbor reports it as the window's `app_menu`.
Check with `arbor msg --json windows` and
`gdbus call --session --dest <service> --object-path <path> --method com.canonical.dbusmenu.GetLayout -- 0 -1 '[]'`.

## Terminal

The update's terminal (`src/terminal_widget.{h,cpp}`) is a QTermWidget kept
off screen and painted into the QML scene, with input forwarded to it. Don't
go back to a native child window (WindowContainer): on Wayland that is a
subsurface mapped apart from the window's frames, and it flickers.

## Apps manifest and AppImages

`nix-apps/` is a workspace crate (library + `nix-apps` CLI) for the flake's
`apps.json`: what the system installs from nixpkgs, Flatpak and as AppImages.
`nix/apps.nix` (exported as `nixosModules.apps`) is the NixOS module that
installs it; /etc/nixos imports it from a `flake = false` input. A store app
is meant to share both.

- AppImages are pinned by URL and hash and name a release source (GitHub,
  GitLab, Electron feed, zsync, download page). Versioned URLs are why this
  isn't flake inputs.
- The checker lists newer releases next to Flatpaks; the update script runs
  this binary as `nixos-update-checker apps update IDs` (as the flake's owner)
  to pin them, then rebuilds and commits apps.json with flake.lock.
- Never execute an AppImage to inspect it: binfmt sends even
  `--appimage-offset` through appimage-run, which starts the app. `appimage.rs`
  reads the ELF and the SquashFS (unsquashfs) instead.
- apps.json must be tracked by git, or the flake doesn't see it.
