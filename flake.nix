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
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        nativeBuildInputs = with pkgs; [
          rustToolchain
          pkg-config
          cmake
          ninja
          qt6.wrapQtAppsHook
          qt6.qtdeclarative  # provides qmltyperegistrar
        ];

        buildInputs = with pkgs; [
          qt6.qtbase
          qt6.qtdeclarative
          qt6.qtwayland
          qt6.qtsvg  # for SVG icon support
          qt6.qttools  # for Qt.labs.platform
          libGL
        ];

        # Runtime dependencies for the update script
        runtimeDeps = with pkgs; [
          nix
          git
          coreutils
        ];

        # Create a combined Qt environment with all necessary tools
        # qt-build-utils looks for tools relative to qmake, so we need to create
        # a unified tree where qmake can find qmltyperegistrar
        qtCombined = pkgs.symlinkJoin {
          name = "qt6-combined";
          paths = [
            pkgs.qt6.qtbase
            pkgs.qt6.qtdeclarative
            pkgs.qt6.qtsvg
            pkgs.qt6.qtwayland
            pkgs.qt6.qttools
          ];
        };

        # Qt libexec path containing qmltyperegistrar
        qtLibexec = "${pkgs.qt6.qtdeclarative}/libexec";

        # Create a wrapper qmake that reports correct paths for the combined Qt tree
        qmakeWrapper = pkgs.writeShellScriptBin "qmake" ''
          # If querying paths, return the combined tree paths
          if [[ "$*" == *"-query"* ]]; then
            case "$*" in
              *QT_HOST_LIBEXECS*|*QT_INSTALL_LIBEXECS*)
                echo "${qtCombined}/libexec"
                exit 0
                ;;
              *QT_HOST_BINS*|*QT_INSTALL_BINS*)
                echo "${qtCombined}/bin"
                exit 0
                ;;
              *QT_HOST_PREFIX*|*QT_INSTALL_PREFIX*)
                echo "${qtCombined}"
                exit 0
                ;;
            esac
          fi
          # For other operations, delegate to real qmake
          exec ${pkgs.qt6.qtbase}/bin/qmake "$@"
        '';

        # Create a wrapped cargo that has access to Qt tools and correct QMAKE
        cargoWrapped = pkgs.writeShellScriptBin "cargo" ''
          export PATH="${qmakeWrapper}/bin:${qtCombined}/libexec:${qtLibexec}:$PATH"
          export QMAKE="${qmakeWrapper}/bin/qmake"
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

          inherit nativeBuildInputs buildInputs;

          # Ensure Qt plugins are found at runtime
          postInstall = ''
            wrapProgram $out/bin/nixos-update-checker \
              --prefix PATH : ${pkgs.lib.makeBinPath runtimeDeps}
          '';

          QMAKE = "${pkgs.qt6.qtbase.dev}/bin/qmake";

          meta = with pkgs.lib; {
            description = "System tray app for monitoring NixOS flake updates";
            license = licenses.mit;
            platforms = platforms.linux;
          };
        };

        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs;

          # Put wrapped cargo first so it takes precedence, then the rest
          packages = [ cargoWrapped ] ++ runtimeDeps ++ [ pkgs.rust-analyzer rustToolchain ];

          inherit buildInputs;

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

          # Qt environment variables
          QT_QPA_PLATFORM = "wayland;xcb";

          # Qt tool paths for cxx-qt build - use the combined Qt tree with wrapper qmake
          QMAKE = "${qmakeWrapper}/bin/qmake";
          QT_HOST_PATH = "${qtCombined}";

          shellHook = ''
            echo "NixOS Update Checker development environment"
            echo "Run 'cargo build' to build the project"
            export PATH="${qmakeWrapper}/bin:${qtCombined}/libexec:${qtLibexec}:$PATH"
            export QMAKE="${qmakeWrapper}/bin/qmake"
          '';
        };

        # NixOS module for easy integration
        nixosModules.default = { config, lib, pkgs, ... }:
          with lib;
          let
            cfg = config.services.nixos-update-checker;
          in
          {
            options.services.nixos-update-checker = {
              enable = mkEnableOption "NixOS Update Checker";

              flakePath = mkOption {
                type = types.path;
                description = "Path to the NixOS flake configuration";
              };

              checkIntervalMinutes = mkOption {
                type = types.int;
                default = 60;
                description = "How often to check for updates (in minutes)";
              };

              terminal = mkOption {
                type = types.str;
                default = "ghostty";
                description = "Terminal emulator to use for running updates";
              };
            };

            config = mkIf cfg.enable {
              environment.systemPackages = [ self.packages.${system}.default ];

              # Create default config
              environment.etc."xdg/nixos-update-checker/config.toml".text = ''
                flake_path = "${cfg.flakePath}"
                check_interval_minutes = ${toString cfg.checkIntervalMinutes}
                terminal = "${cfg.terminal}"
              '';
            };
          };
      }
    );
}
