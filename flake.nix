{
  description = "ripgrep, but also search in PDFs, E-Books, Office documents, zip, tar.gz, etc.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };

    pre-commit-hooks = {
      url = "github:cachix/pre-commit-hooks.nix";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    flake-utils,
    rust-overlay,
    advisory-db,
    pre-commit-hooks,
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };

      craneLib = crane.lib.${system};
      src = pkgs.lib.cleanSourceWith {
        src = craneLib.path ./.;
        filter = pkgs.lib.cleanSourceFilter;
      };

      nativeBuildInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin [
        # Additional darwin specific inputs can be set here
        pkgs.libiconv
      ];

      runtimeInputs = with pkgs; [ffmpeg pandoc poppler_utils ripgrep zip];

      # Build *just* the cargo dependencies, so we can reuse
      # all of that work (e.g. via cachix) when running in CI
      cargoArtifacts =
        craneLib.buildDepsOnly {inherit src nativeBuildInputs;};

      # Build the actual crate itself, reusing the dependency
      # artifacts from above.
      rgaBinary = craneLib.buildPackage {
        inherit cargoArtifacts src nativeBuildInputs;
        buildInputs = runtimeInputs; # needed for tests
      };

      # Provide a shell script of the Rust binary plus runtime dependencies.
      rga = pkgs.pkgs.writeShellApplication {
        name = "rga";
        text = ''rga "$@"'';
        runtimeInputs = runtimeInputs ++ [rgaBinary];
      };

      pre-commit = pre-commit-hooks.lib."${system}".run;
    in {
      # `nix flake check`
      checks = {
        # Build the crate as part of `nix flake check` for convenience
        inherit rgaBinary;

        # Run clippy (and deny all warnings) on the crate source,
        # again, resuing the dependency artifacts from above.
        #
        # Note that this is done as a separate derivation so that
        # we can block the CI if there are issues here, but not
        # prevent downstream consumers from building our crate by itself.
        rga-clippy = craneLib.cargoClippy {
          inherit cargoArtifacts src;
          cargoClippyExtraArgs = "--all-targets -- --deny warnings";
        };

        rga-doc = craneLib.cargoDoc {inherit cargoArtifacts src;};

        # Check formatting
        rga-fmt = craneLib.cargoFmt {inherit src;};

        # Audit dependencies
        rga-audit = craneLib.cargoAudit {inherit src advisory-db;};

        # Run tests with cargo-nextest.
        rga-nextest = craneLib.cargoNextest {
          inherit cargoArtifacts src nativeBuildInputs;
          buildInputs = runtimeInputs; # needed for tests
          partitions = 1;
          partitionType = "count";
        };

        pre-commit = pre-commit {
          src = ./.;
          hooks = {
            alejandra.enable = true;
            rustfmt.enable = true;
            typos = {
              enable = true;
              settings = {
                exclude = "exampledir/*";
              };
            };
          };
        };
      };

      # `nix build`
      packages = {
        inherit rgaBinary rga;
        default = rga; # `nix build`
      };

      # `nix run`
      apps.default = flake-utils.lib.mkApp {drv = rga;};

      # `nix develop`
      devShells.default = craneLib.devShell {
        inherit (self.checks.${system}.pre-commit) shellHook;
        inputsFrom = builtins.attrValues self.checks;
        packages = runtimeInputs ++ nativeBuildInputs;
      };
    });
}
