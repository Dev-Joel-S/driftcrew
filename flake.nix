{
  # NixOS: `nix run .` baut und startet das Spiel, `nix develop` öffnet eine Shell mit allem,
  # was `cargo run --release` braucht. Ein normales Linux-Binary läuft auf NixOS nicht direkt,
  # weil Vulkan, ALSA, udev und X11/Wayland dort nicht unter den üblichen Pfaden liegen.
  description = "DriftCrew – Koop-Weltraumspiel: jede Taste ist ein Triebwerk, der Rest ist Physik";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      # Wird gelinkt bzw. zur Laufzeit per dlopen geladen.
      runtimeLibs =
        pkgs: with pkgs; [
          alsa-lib
          udev
          vulkan-loader
          libxkbcommon
          wayland
          libx11
          libxcursor
          libxi
          libxrandr
        ];
    in
    {
      packages = forAll (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "driftcrew";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.makeWrapper
          ];
          buildInputs = runtimeLibs pkgs;
          doCheck = false;
          postFixup = ''
            wrapProgram $out/bin/driftcrew \
              --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath (runtimeLibs pkgs)}
          '';
          meta.mainProgram = "driftcrew";
        };
      });

      apps = forAll (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/driftcrew";
        };
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            pkg-config
          ];
          buildInputs = runtimeLibs pkgs;
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (runtimeLibs pkgs);
        };
      });
    };
}
