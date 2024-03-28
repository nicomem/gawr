{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";

    crane.url = "github:ipetkov/crane";
    crane.inputs.nixpkgs.follows = "nixpkgs";

    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { nixpkgs, crane, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        craneLib = crane.lib.${system};
      in
      {
        packages.default = craneLib.buildPackage {
          src = craneLib.cleanCargoSource (craneLib.path ./.);
        };

        devShells.default = craneLib.devShell {
          # --- Environment variables ----
          # Needed for rust-analyzer
          RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";

          # --- Extra packages ---
          packages = [
            pkgs.bashInteractive
            pkgs.yt-dlp
          ];
        };
      });
}
