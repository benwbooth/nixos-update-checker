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
          kdePackages.qqc2-desktop-style
          kdePackages.plasma-integration
          kdePackages.breeze
          kdePackages.kio
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

          nativeBuildInputs = nativeBuildInputs ++ [ pkgs.copyDesktopItems pkgs.qt6.wrapQtAppsHook pkgs.mold pkgs.rsync ];
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
            # Pass git hash to build.rs
            export BUILD_GIT_HASH="${self.shortRev or self.dirtyShortRev or "dirty"}"
          '';

          # Build in a persistent directory so dev and system builds keep
          # cargo's incremental state. Falls back to a throwaway directory
          # when the sandbox doesn't expose the cache.
          doCheck = false;
          buildPhase = ''
            runHook preBuild
            umask 000
            workDir=$TMPDIR/cargo-nix
            if [ -w /var/tmp/cargo-nix ]; then
              workDir=/var/tmp/cargo-nix/nixos-update-checker
            fi
            mkdir -p "$workDir/src" "$workDir/target"
            touch "$workDir"
            # Build scripts copy read-only headers out of the store.
            chmod -R a+rwX "$workDir/target"
            # Store paths have a fixed mtime, so only rewrite files whose
            # content changed; cargo's freshness check relies on that.
            rsync -rc --no-times --no-perms --delete ./ "$workDir/src/"
            # The per-build copy gets new mtimes, which would rebuild every
            # dependency; the store path never changes.
            sed -i "s|^directory = .*|directory = \"$cargoDeps\"|" "$NIX_BUILD_TOP/.cargo/config.toml"
            # The linker wrapper rejects output paths outside the build dir.
            export NIX_ENFORCE_PURITY=0
            (
              cd "$workDir/src"
              # Vendored deps are configured in $NIX_BUILD_TOP/.cargo, above the source.
              CARGO_HOME="$NIX_BUILD_TOP/.cargo" RUSTFLAGS="-C link-arg=-fuse-ld=mold" CARGO_TARGET_DIR="$workDir/target" \
                cargo build --release --offline --locked -j "$NIX_BUILD_CORES"
            )
            install -Dm755 "$workDir/target/release/nixos-update-checker" bin/nixos-update-checker
            runHook postBuild
          '';
          installPhase = ''
            runHook preInstall
            install -Dm755 bin/nixos-update-checker $out/bin/nixos-update-checker
            runHook postInstall
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

            # Install icon to hicolor theme (SVG for scalable)
            mkdir -p $out/share/icons/hicolor/scalable/apps
            cp resources/icons/nix-flake.svg $out/share/icons/hicolor/scalable/apps/nixos-update-checker.svg

            # Install PNG icons at standard sizes (KDE prefers these)
            for size in 16 24 32 48 64 128 256; do
              mkdir -p $out/share/icons/hicolor/''${size}x''${size}/apps
              cp resources/icons/png/nixos-update-checker-''${size}.png $out/share/icons/hicolor/''${size}x''${size}/apps/nixos-update-checker.png
            done
          '';

          # Desktop file uses Icon=nixos-update-checker
          # KDE will find PNG icons in hicolor theme at standard sizes

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

          # Keep flake inputs in closure so GC doesn't collect them
          FLAKE_INPUTS = builtins.concatStringsSep ":" [ "${nixpkgs}" "${rust-overlay}" "${flake-utils}" ];

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
          inherit (pkgs.stdenv.hostPlatform) system;
          pkg = cfg.package;
          desktopItem = pkgs.makeDesktopItem {
            name = "nixos-update-checker-autostart";
            exec = "${pkg}/bin/nixos-update-checker";
            icon = "${pkg}/share/icons/hicolor/scalable/apps/nixos-update-checker.svg";
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
