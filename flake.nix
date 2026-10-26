{
  description = "a Night Fury that lives in the corner of your Wayland desktop";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: rec {
        toothless = pkgs.rustPlatform.buildRustPackage {
          pname = "toothless";
          version = (fromTOML (builtins.readFile ./Cargo.toml)).package.version;

          src = self;
          cargoLock.lockFile = ./Cargo.lock;

          meta = {
            description = "A Night Fury that lives in the corner of your Wayland desktop";
            license = pkgs.lib.licenses.mit;
            mainProgram = "toothless";
            platforms = pkgs.lib.platforms.linux;
          };
        };

        default = toothless;
      });

      homeModules = rec {
        toothless = import ./nix/home-manager.nix self;
        default = toothless;
      };

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
          ];

          env = {
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
            RUST_BACKTRACE = "1";
          };
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt-tree);
    };
}
