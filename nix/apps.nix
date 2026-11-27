# Installs what an apps manifest lists (apps.json; the format is documented
# in nix-apps/src/manifest.rs): packages from nixpkgs, Flatpak apps (through
# nix-flatpak, when the system uses it) and AppImages.
#
# Programs edit the manifest (the update checker moves AppImages to new
# releases; a store app adds and removes things) and a rebuild applies it.
# Each AppImage is pinned by URL and hash, and becomes a package with its
# own menu entry and icons, wrapped by appimageTools like a nixpkgs AppImage.
#
#   imports = [ "${nixos-update-checker}/nix/apps.nix" ];
#   programs.nix-apps = { enable = true; manifest = ./apps.json; };
{ config, lib, pkgs, options, ... }:
let
  cfg = config.programs.nix-apps;

  manifest =
    if cfg.manifest != null && builtins.pathExists cfg.manifest
    then lib.importJSON cfg.manifest
    else { };

  # "kdePackages.kate" in a package set.
  attr = set: path:
    lib.attrByPath (lib.splitString "." path)
      (throw "apps.json: nixpkgs has no package ${path}")
      set;

  mkAppImage = id: app:
    let
      pname = id;
      inherit (app) version;
      # The same name nix-apps prefetches under (store_name), so the file it
      # downloaded while updating is the one this wants.
      src = pkgs.fetchurl {
        inherit (app) url hash;
        name = lib.strings.sanitizeDerivationName "${id}-${version}.AppImage";
      };
      contents = pkgs.appimageTools.extract { inherit pname version src; };
    in
    pkgs.appimageTools.wrapType2 {
      inherit pname version src;
      extraPkgs = p: map (attr p) (app.extraPkgs or [ ]);

      # The AppImage's own desktop entry (keeping its file name, which
      # Wayland shells match against the app ID), pointed at the wrapper, and
      # its icons.
      extraInstallCommands = ''
        desktop=$(find ${contents} -maxdepth 1 -name '*.desktop' | head -n 1)
        if [ -n "$desktop" ]; then
          entry=$out/share/applications/$(basename "$desktop")
          install -Dm444 "$desktop" "$entry"
          chmod u+w "$entry"
          sed -i -E \
            -e "s|^Exec=\"?[^ \"]*\"?|Exec=$out/bin/${id}|" \
            -e '/^TryExec=/d' \
            "$entry"
          if [ -d ${contents}/usr/share/icons ]; then
            mkdir -p $out/share
            cp -rL --no-preserve=mode ${contents}/usr/share/icons $out/share/
          fi
          icon=$(sed -n 's/^Icon=//p' "$desktop" | head -n 1)
          if [ -n "$icon" ] && ! find $out/share/icons -name "$icon.*" 2>/dev/null | grep -q .; then
            for f in ${contents}/"$icon".svg ${contents}/"$icon".png ${contents}/.DirIcon; do
              if [ -e "$f" ]; then
                case "$f" in
                  *.svg) ext=svg ;;
                  *) ext=png ;;
                esac
                install -Dm444 "$f" $out/share/pixmaps/"$icon".$ext
                break
              fi
            done
          fi
        fi
      '';

      meta = {
        description = app.name;
        homepage = app.homepage or null;
        mainProgram = id;
        platforms = [ "x86_64-linux" ];
        sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
      };
    };

  appimages = lib.mapAttrs mkAppImage (manifest.appimages or { });

  hasNixFlatpak = options ? services && options.services ? flatpak && options.services.flatpak ? packages;
  flatpaks = manifest.flatpaks or [ ];
in
{
  options.programs.nix-apps = {
    enable = lib.mkEnableOption "the apps manifest (apps.json)";

    manifest = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      example = lib.literalExpression "./apps.json";
      description = "The apps manifest. A missing file installs nothing.";
    };

    nixpkgs = lib.mkOption {
      type = lib.types.raw;
      default = pkgs;
      defaultText = lib.literalExpression "pkgs";
      description = "The package set the manifest's nixpkgs names are looked up in.";
    };

    appimages = lib.mkOption {
      type = lib.types.attrsOf lib.types.package;
      readOnly = true;
      default = appimages;
      defaultText = lib.literalMD "the manifest's AppImages, wrapped";
      description = "The packages made from the manifest's AppImages, by id.";
    };
  };

  config = lib.mkIf cfg.enable (lib.mkMerge [
    {
      environment.systemPackages =
        map (attr cfg.nixpkgs) (manifest.nixpkgs or [ ])
        ++ lib.attrValues appimages;

      warnings = lib.optional (flatpaks != [ ] && !hasNixFlatpak)
        "apps.json lists Flatpak apps, but nix-flatpak isn't imported, so they aren't installed.";
    }
    (lib.optionalAttrs hasNixFlatpak {
      services.flatpak.packages = flatpaks;
    })
  ]);
}
