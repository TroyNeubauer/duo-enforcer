{ lib
, darwin
, stdenv
, rustPlatform
, pkg-config
, openssl
}:

rustPlatform.buildRustPackage {
  pname = "duo-enforcer";
  version = "0.1.0";

  src = lib.cleanSource ./.;

  cargoHash = "sha256-wgzW66w6W8g3dVEfBlmPNDLVZ3eIRx4TmZmi4uqE4ic=";

  nativeBuildInputs = [
    pkg-config
  ];

  buildInputs = [
    openssl
  ] ++ lib.optionals stdenv.isDarwin [
    darwin.apple_sdk.frameworks.Foundation
    darwin.apple_sdk.frameworks.SystemConfiguration
  ];
}
