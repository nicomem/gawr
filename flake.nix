{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";

    dream2nix.url = "github:nix-community/dream2nix/legacy";
    dream2nix.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, dream2nix, ... }:
    dream2nix.lib.makeFlakeOutputs {
      systems = ["x86_64-linux"];
      config.projectRoot = ./.;
      source = ./.;
      projects = ./projects.toml;
    };
}
