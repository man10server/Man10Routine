{
  nixConfig = {
    extra-substituters = [
      "https://nix-community.cachix.org"
    ];
    extra-trusted-public-keys = [
      "nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs="
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    fenix.url = "github:nix-community/fenix";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      fenix,
      crane,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };

        rustToolchain = fenix.packages.${system}.fromToolchainFile {
          file = ./rust-toolchain.toml;
          sha256 = "sha256-SDu4snEWjuZU475PERvu+iO50Mi39KVjqCeJeNvpguU=";
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        src = craneLib.cleanCargoSource ./.;
        commonArgs = {
          inherit src;
          strictDeps = true;
          buildInputs =
            [ ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin (
              with pkgs;
              [
                libiconv
              ]
            );
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        crate = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
          }
        );
      in
      {

        formatter = pkgs.writeShellApplication {
          name = "man10-routine-formatter";
          runtimeInputs = with pkgs; [
            nixfmt-rfc-style
            rustToolchain
          ];
          text = ''
            fd "$@" -t f -e nix -x nixfmt '{}'
            cargo fmt
          '';
        };

        checks = {
          inherit crate;

          nix-fmt =
            pkgs.runCommand "check-nix-formatting"
              {
                buildInputs = with pkgs; [
                  nixfmt-rfc-style
                  fd
                  gitMinimal
                ];
                src = self;
              }
              ''
                cp -r --no-preserve=mode $src src
                cd src
                git init --quiet && git add .
                fd "$@" -t f -e nix -x nixfmt '{}'
                if ! git diff --exit-code; then
                  exit 1
                fi
                touch $out
              '';

          crate-clippy = craneLib.cargoClippy (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            }
          );

          crate-fmt = craneLib.cargoFmt {
            inherit src;
          };
        };

        devShells.default = pkgs.mkShell {
          name = "man10-routine-dev";
          buildInputs = [
            rustToolchain
            fenix.packages.${system}.rust-analyzer
          ];
        };

        packages.default = crate;

        apps.default = flake-utils.lib.mkApp {
          drv = crate;
        };
      }
    );
}
