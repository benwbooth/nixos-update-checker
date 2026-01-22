{
  description = "NixOS Update Checker - System tray app for monitoring flake updates";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    (flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        # Create a proper Qt environment using qt6.env
        # This creates a unified tree where qmake returns correct paths
        qtEnv = pkgs.qt6.env "qt-env" [
          pkgs.qt6.qtdeclarative
          pkgs.qt6.qtsvg
          pkgs.qt6.qtwayland
          pkgs.qt6.qttools
        ];

        # QTermWidget for embedded terminal
        qtermwidget = pkgs.lxqt.qtermwidget;

        nativeBuildInputs = with pkgs; [
          rustToolchain
          pkg-config
          cmake
          ninja
        ];

        buildInputs = with pkgs; [
          qt6.qtbase
          qt6.qtdeclarative
          qt6.qtwayland
          qt6.qtsvg
          qt6.qttools
          libGL
          lxqt.qtermwidget
        ];

        # Runtime dependencies for the update script
        runtimeDeps = with pkgs; [
          nix
          git
          coreutils
        ];

        # Create a wrapped cargo that uses the qtEnv qmake
        cargoWrapped = pkgs.writeShellScriptBin "cargo" ''
          export PATH="${qtEnv}/bin:${qtEnv}/libexec:$PATH"
          export QMAKE="${qtEnv}/bin/qmake"
          exec ${rustToolchain}/bin/cargo "$@"
        '';
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "nixos-update-checker";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = nativeBuildInputs ++ [ pkgs.copyDesktopItems pkgs.qt6.wrapQtAppsHook ];
          inherit buildInputs;

          # Don't use cmake/ninja as the main build system - they're only needed for cxx-qt internally
          dontUseCmakeConfigure = true;
          dontUseNinjaBuild = true;
          dontUseNinjaInstall = true;

          # Qt environment for cxx-qt build - use qtEnv which has proper qmake paths
          QMAKE = "${qtEnv}/bin/qmake";

          preBuild = ''
            # Override QMAKE that wrapQtAppsHook sets - we need our qtEnv qmake
            export QMAKE="${qtEnv}/bin/qmake"
            export PATH="${qtEnv}/libexec:$PATH"
            # Qt paths for build script
            export QT_INCLUDE_PATH="${qtEnv}/include"
            export QT_LIBEXEC_PATH="${qtEnv}/libexec"
            # QTermWidget paths for embedding terminal
            export QTERMWIDGET_INCLUDE_PATH="${qtermwidget}/include"
            export QTERMWIDGET_LIB_PATH="${qtermwidget}/lib"
          '';

          desktopItems = [
            (pkgs.makeDesktopItem {
              name = "nixos-update-checker";
              exec = "nixos-update-checker";
              icon = "nixos-update-checker";
              desktopName = "NixOS Update Checker";
              comment = "Monitor NixOS flake repository for updates";
              categories = [ "System" "Monitor" ];
              keywords = [ "nixos" "nix" "update" "flake" ];
              startupNotify = false;
              extraConfig = {
                StartupWMClass = "nixos-update-checker";
              };
            })
          ];

          postInstall = ''
            wrapProgram $out/bin/nixos-update-checker \
              --prefix PATH : ${pkgs.lib.makeBinPath runtimeDeps}:$out/bin

            # Install the update script
            install -Dm755 scripts/nixos-update-checker-update $out/bin/nixos-update-checker-update

            # Install polkit policy for clean pkexec dialog
            install -Dm644 scripts/org.nixos-update-checker.policy $out/share/polkit-1/actions/org.nixos-update-checker.policy

            # Install icon to hicolor theme
            mkdir -p $out/share/icons/hicolor/scalable/apps
            cp resources/icons/nix-flake.svg $out/share/icons/hicolor/scalable/apps/nixos-update-checker.svg
          '';

          meta = with pkgs.lib; {
            description = "System tray app for monitoring NixOS flake updates";
            license = licenses.mit;
            platforms = platforms.linux;
          };
        };

        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs;

          # Put wrapped cargo first so it takes precedence, then the rest
          packages = [ cargoWrapped ] ++ runtimeDeps ++ [ pkgs.rust-analyzer rustToolchain pkgs.cargo-watch ];

          inherit buildInputs;

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

          # Qt environment variables
          QT_QPA_PLATFORM = "wayland;xcb";

          # Use qtEnv for proper qmake paths
          QMAKE = "${qtEnv}/bin/qmake";

          shellHook = ''
            echo "NixOS Update Checker development environment"
            echo "Run 'cargo build' to build the project"
            export PATH="${qtEnv}/bin:${qtEnv}/libexec:$PWD/scripts:$PATH"
            export QMAKE="${qtEnv}/bin/qmake"
            # QTermWidget paths for embedding terminal
            export QTERMWIDGET_INCLUDE_PATH="${qtermwidget}/include"
            export QTERMWIDGET_LIB_PATH="${qtermwidget}/lib"
          '';
        };

      }
    )) // {
      # NixOS module for easy integration (outside eachDefaultSystem)
      nixosModules.default = { config, lib, pkgs, ... }:
        with lib;
        let
          cfg = config.programs.nixos-update-checker;
          system = pkgs.stdenv.hostPlatform.system;
          pkg = cfg.package;
          desktopItem = pkgs.makeDesktopItem {
            name = "nixos-update-checker-autostart";
            exec = "${pkg}/bin/nixos-update-checker";
            icon = "nixos-update-checker";
            desktopName = "NixOS Update Checker";
            comment = "Monitor NixOS flake repository for updates";
            categories = [ "System" "Monitor" ];
            startupNotify = false;
          };
        in
        {
          options.programs.nixos-update-checker = {
            enable = mkEnableOption "NixOS Update Checker";

            package = mkOption {
              type = types.package;
              default = self.packages.${system}.default;
              description = "The nixos-update-checker package to use";
            };

            autostart = mkOption {
              type = types.bool;
              default = true;
              description = "Whether to autostart on user login";
            };
          };

          config = mkIf cfg.enable {
            environment.systemPackages = [ pkg ];

            # XDG autostart entry for user session
            environment.etc."xdg/autostart/nixos-update-checker.desktop".source =
              mkIf cfg.autostart "${desktopItem}/share/applications/nixos-update-checker-autostart.desktop";
          };
        };
    };
}
