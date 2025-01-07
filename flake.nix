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
        duo-enforcer = import ./package.nix {
          inherit (pkgs) lib darwin stdenv rustPlatform pkg-config openssl;
        };
      in {
        devShells = rec {
          default = pkgs.mkShell {
            CARGO_NET_GIT_FETCH_WITH_CLI = "true";
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ pkgs.openssl ];

            packages = with pkgs; [
              pkg-config
              openssl
            ] ++ lib.optionals stdenv.isDarwin [
              darwin.apple_sdk.frameworks.Foundation
              darwin.apple_sdk.frameworks.SystemConfiguration
            ];
          };
        };

        packages = {
          inherit duo-enforcer;
        };
      });
}
