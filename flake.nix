{
  description = "bubblepolicy flake";

  inputs.nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
  inputs.flake-parts.url = "github:hercules-ci/flake-parts";
  inputs.rust-overlay = {
    url = "github:oxalica/rust-overlay?ref=stable";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    { flake-parts, ... }@inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {

      systems = [
        "aarch64-linux"
        "x86_64-linux"
      ];

      perSystem =
        { pkgs, system, ... }:
        {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [
              inputs.rust-overlay.overlays.default
            ];
          };

          packages = rec {
            default = bubblepolicy;
            bubblepolicy = pkgs.callPackage ./package.nix { };
          };

          devShells.default = pkgs.mkShell {
            packages = with pkgs; [
              pkg-config
              rustPlatform.bindgenHook
              strace
              bubblewrap
              # Rust toolchain with rust-src (for procedural macros) and rust-analyzer
              (rust-bin.stable.latest.default.override {
                extensions = [
                  "rust-src"
                  "rust-analyzer"
                ];
                targets = [ ];
              })
            ];
          };

          # Use nixfmt for all nix files
          formatter = pkgs.nixfmt-tree;
        };
    };
}
