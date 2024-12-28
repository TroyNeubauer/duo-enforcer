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
          targets = [ "wasm32-unknown-unknown" ];
        };
        wasmBindgenCliPackage = import "${inputs.nixpkgs}/pkgs/by-name/wa/wasm-bindgen-cli/package.nix" {
          inherit (pkgs) lib rustPlatform fetchCrate nix-update-script nodejs_latest
            pkg-config openssl stdenv curl darwin;
          version = "0.2.99";
          hash = "sha256-1AN2E9t/lZhbXdVznhTcniy+7ZzlaEp/gwLEAucs6EA=";
          cargoHash = "sha256-DbwAh8RJtW38LJp+J9Ht8fAROK9OabaJ85D9C/Vkve4=";
        };
        # See: https://github.com/NixOS/nixpkgs/blob/nixos-24.11/pkgs/by-name/ca/cargo-leptos/package.nix
        cargoLeptosPackage = pkgs.rustPlatform.buildRustPackage rec {
          pname = "cargo-leptos";
          version = "0.2.24";
          src = pkgs.fetchFromGitHub {
            owner = "leptos-rs";
            repo = pname;
            rev = "v${version}";
            hash = "sha256-KNasxy/hEU04H/xAV3y92OtxBmYgA0RZjCQnC/XvBoo=";
          };
          cargoHash = "sha256-dxDmJVkkdT6hbhboyn8XwMJp379xAVZ8GFVp3F+LtWA=";
          buildInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin (
            with pkgs.darwin.apple_sdk.frameworks; [
              SystemConfiguration
              Security
              CoreServices
            ]);
          buildFeatures = [ "no_downloads" ];
          doCheck = false;
        };
      in {
        devShells = rec {
          default = pkgs.mkShell {
            CARGO_NET_GIT_FETCH_WITH_CLI = "true";

            packages = with pkgs; [
              rust-pkgs
              cargoLeptosPackage
              wasmBindgenCliPackage
              sass
            ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin (
              with pkgs.darwin.apple_sdk.frameworks; [
                Foundation
                SystemConfiguration
            ]);
          };
        };
      });
}

