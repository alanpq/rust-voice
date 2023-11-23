{
  inputs = {
    zicross.url = "github:flyx/Zicross";
    fenix.url = "github:nix-community/fenix";
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, zicross, fenix, nixpkgs, utils, naersk }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            zicross.overlays.zig
            zicross.overlays.windows
          ];
        };
        lib = pkgs.lib;


        fetchMsys = { tail, sha256, ... }:
          builtins.fetchurl {
            url =
              "https://mirror.msys2.org/mingw/clang64/mingw-w64-clang-x86_64-${tail}";
              inherit sha256;
          };
        
        pkgsFromPacman = name: input: let 
            src = fetchMsys input;
          in pkgs.stdenvNoCC.mkDerivation ((builtins.removeAttrs input [ "tail" "sha256" ]) // {
            name = "msys2-${name}";
            inherit src;
            phases = [ "unpackPhase" "patchPhase" "installPhase" ];
            nativeBuildInputs = with pkgs; [ gnutar zstd ];
            unpackPhase = ''
              runHook preUnpack
              mkdir -p upstream
              ${pkgs.gnutar}/bin/tar -xvpf $src -C upstream \
              --exclude .PKGINFO --exclude .INSTALL --exclude .MTREE --exclude .BUILDINFO
              runHook postUnpack
            '';
            patchPhase = ''
              runHook prePatch
              shopt -s globstar
              find -type f -name "*.a" -not -name "*.dll.a" -not -name "*main.a" -delete
              runHook postPatch
            '';
            installPhase = ''
              runHook preInstall
              mkdir -p $out/
              cp -rt $out upstream/*
              runHook postInstall
            '';
        });

        toolchain = with fenix.packages.${system};
          combine [
            minimal.rustc
            minimal.cargo
            targets.x86_64-pc-windows-gnu.latest.rust-std
          ];
        naersk-lib = naersk.lib.${system}.override {
          cargo = toolchain;
          rustc = toolchain;
        };
        buildPackage = target: { nativeBuildInputs ? [], ...}@args:
          naersk-lib.buildPackage ({
            src = ./.;
            strictDeps = true;
          } // (lib.optionalAttrs (target != system) {
            CARGO_BUILD_TARGET = target;
          }) // args // {
            nativeBuildInputs = with pkgs; [
              pkg-config
              libopus
            ] ++ nativeBuildInputs;
          });
      in rec
      {
        packages = let
          windows = buildPackage "x86_64-pc-windows-gnu" rec {
            doCheck = system == "x86_64_linux";
            depsBuildBuild = with pkgs; [
              libopus
              pkgsCross.mingwW64.libopus
              pkgsCross.mingwW64.stdenv.cc
              pkgsCross.mingwW64.windows.pthreads
            ];

            nativeBuildInputs = lib.optional doCheck pkgs.wineWowPackages.stable;

            #OPUS_NO_PKG="1";
            #OPUS_STATIC="1";
            OPUS_LIB_DIR="${pkgs.libopus}";
            CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER = pkgs.writeScript "wine-wrapper" ''
              # Without this, wine will error out when attempting to create the
              # prefix in the build's homeless shelter.
              export WINEPREFIX="$(mktemp -d)"
              exec wine64 $@
            '';
          };

         in {
          default = buildPackage system { };
          x86_64-unknown-linux-musl = buildPackage "x86_64-unknown-linux-musl" {
            nativeBuildInputs = with pkgs; [pkgsStatic.stdenv.cc];
            CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS = "-C target-feature=+crt-static";
          };

          x86_64-pc-windows-gnu = let
            deps = {
              libopus = {
                tail = "opus-1.4-2-any.pkg.tar.zst";
                sha256 = "0zp5vbb5pj1qhdrqlyhc9dzsaixnjls82zx3c6x174miqb4f4j5z";
              };
            };
            bundled = windows.overrideAttrs (origAttrs: {
              buildInputs = lib.mapAttrsToList pkgsFromPacman deps;
              # (origAttrs.postInstall or "") + ''
              postInstall = ''
                for item in $buildInputs; do
                  cp -t $out/bin $item/clang64/bin/*.dll | true # allow deps without dlls
                done
              '';
            });
          in pkgs.stdenvNoCC.mkDerivation {
            name = "${bundled.name}-win64.zip";
            unpackPhase = ''
              packDir=${bundled.name}-win64
              mkdir -p $packDir
              cp -rt $packDir --no-preserve=mode ${bundled}/*
            '';
            buildPhase = ''
              ${pkgs.zip}/bin/zip -r $packDir.zip $packDir
            '';
            installPhase = ''
              cp $packDir.zip $out
            '';
          };
        };

        apps.default = {
          type = "app";
          program = "${packages.default}/bin/curses";
        };

        devShell = with pkgs; mkShell {
          buildInputs = [ 
            cargo rustc rustfmt pre-commit rustPackages.clippy 
            rust-analyzer
            pkg-config alsa-lib 
            libopus
            ncurses jack2 libjack2
            wineWowPackages.stable
          ];
          RUST_SRC_PATH = rustPlatform.rustLibSrc;
        };
      });
}
