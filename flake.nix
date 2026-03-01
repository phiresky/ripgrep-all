{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
    git-hooks-nix = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nix-systems = {
      url = "github:nix-systems/default";
      flake = false;
    };
    rust-flake = {
      url = "github:juspay/rust-flake";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{
      flake-parts,
      nix-systems,
      ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } (
      top@{
        config,
        withSystem,
        moduleWithSystem,
        ...
      }:
      {
        imports = [
          inputs.git-hooks-nix.flakeModule
          inputs.rust-flake.flakeModules.default
          inputs.rust-flake.flakeModules.nixpkgs
          inputs.treefmt-nix.flakeModule
        ];
        systems = import nix-systems;
        perSystem =
          {
            config,
            self',
            pkgs,
            ...
          }:
          {
            pre-commit = {
              check.enable = true;
              settings.hooks = {
                ripsecrets.enable = true;
                treefmt.enable = true;
                typos = {
                  enable = true;
                  settings.exclude = "exampledir/*";
                };
              };
            };
            rust-project =
              let
                dependencies = with pkgs; [
                  ffmpeg
                  pandoc
                  poppler_utils
                  ripgrep
                  zip
                ];
              in
              {
                src = pkgs.lib.cleanSourceWith {
                  src = config.rust-project.crane-lib.path ./.;
                  filter = pkgs.lib.cleanSourceFilter;
                };
                crates.ripgrep_all.crane.args.buildInputs = dependencies;
                crates.ripgrep_all.crane.args.nativeBuildInputs = dependencies;
              };
            treefmt.config = {
              projectRootFile = ".git/config";
              flakeCheck = false; # use pre-commit's check instead
              programs = {
                nixfmt.enable = true;
                rustfmt = {
                  enable = true;
                  package = config.rust-project.crane-lib.rustfmt;
                };
              };
            };
            packages = {
              default = self'.packages.ripgrep_all;
              ripgrep-all = self'.packages.ripgrep_all;
              rga = self'.packages.ripgrep_all;
            };
            devShells.default = config.rust-project.crane-lib.devShell {
              inputsFrom = [
                config.packages.default
                config.pre-commit.devShell
                config.treefmt.build.devShell
              ];
            };
          };
      }
    );
}
