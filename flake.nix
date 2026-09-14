{
  description = "Minimal MCP protocol compatibility shim for Nexus";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane.url = "github:ipetkov/crane";

    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      flake-utils,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachSystem
      [ "x86_64-linux" ]
      (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };

          rustToolchain =
            pkgs.rust-bin.fromRustupToolchainFile
              ./rust-toolchain.toml;

          craneLib =
            (crane.mkLib pkgs).overrideToolchain
              rustToolchain;

          src = craneLib.cleanCargoSource ./.;

          commonArgs = {
            inherit src;
            strictDeps = true;
          };

          cargoArtifacts =
            craneLib.buildDepsOnly commonArgs;

          package =
            craneLib.buildPackage (
              commonArgs
              // {
                inherit cargoArtifacts;
              }
            );

          staticArgs =
            commonArgs
            // {
              CARGO_BUILD_TARGET =
                "x86_64-unknown-linux-musl";

              CARGO_BUILD_RUSTFLAGS =
                "-C target-feature=+crt-static";
            };

          staticCargoArtifacts =
            craneLib.buildDepsOnly staticArgs;

          staticPackage =
            craneLib.buildPackage (
              staticArgs
              // {
                cargoArtifacts =
                  staticCargoArtifacts;
              }
            );

          ociImage =
            pkgs.dockerTools.buildLayeredImage {
              name =
                "ghcr.io/blogle/nexus-mcp-compat";

              tag = "latest";

              contents = [ staticPackage ];

              config = {
                Entrypoint = [
                  "${staticPackage}/bin/nexus-mcp-compat"
                ];

                User = "65532:65532";

                ExposedPorts = {
                  "8000/tcp" = { };
                };

                Env = [
                  "LISTEN_ADDR=0.0.0.0:8000"
                  "RUST_LOG=info"
                ];

                Labels = {
                  "org.opencontainers.image.title" =
                    "nexus-mcp-compat";

                  "org.opencontainers.image.description" =
                    "Minimal MCP 2026-07-28 to legacy Nexus compatibility shim";

                  "org.opencontainers.image.source" =
                    "https://github.com/blogle/nexus-mcp-compat";

                  "org.opencontainers.image.licenses" =
                    "MIT";
                };
              };
            };
        in
        {
          packages = {
            default = staticPackage;
            host = package;
            oci = ociImage;
          };

          apps.default =
            flake-utils.lib.mkApp {
              drv = staticPackage;
            };

          checks = {
            formatting =
              craneLib.cargoFmt {
                inherit src;
              };

            clippy =
              craneLib.cargoClippy (
                commonArgs
                // {
                  inherit cargoArtifacts;

                  cargoClippyExtraArgs =
                    "--all-targets -- --deny warnings";
                }
              );

            tests =
              craneLib.cargoTest (
                commonArgs
                // {
                  inherit cargoArtifacts;
                }
              );
          };

          devShells.default =
            craneLib.devShell {
              checks = self.checks.${system};

              packages = with pkgs; [
                curl
                git
                gh
                jq
              ];
            };
        }
      );
}
