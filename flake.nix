{
  description = "Rust flake";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs?rev=4005c3ff7505313cbc21081776ad0ce5dfd7a3ce";

    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, ... }@inputs:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            (import rust-overlay)
          ];
        };
        rust-pkgs = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-analyzer" "rust-src" ];
        };
      in {
        devShells = rec {
          default = pkgs.mkShell {
            CARGO_NET_GIT_FETCH_WITH_CLI = "true";

            packages = with pkgs; [
              rust-pkgs
            ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin (
              with pkgs.darwin.apple_sdk.frameworks; [
                Foundation
                SystemConfiguration
            ]);
          };
        };
      });
}

