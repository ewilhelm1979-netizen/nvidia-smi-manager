{
  description = "Native Rust NVIDIA GPU Manager for NixOS";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
      overlay = final: prev: {
        nvidia-gpu-manager = final.callPackage ./nix/package.nix { };
      };
    in
    {
      overlays.default = overlay;

      packages = forAllSystems (system:
        let pkgs = import nixpkgs { inherit system; overlays = [ overlay ]; };
        in {
          default = pkgs.nvidia-gpu-manager;
          nvidia-gpu-manager = pkgs.nvidia-gpu-manager;
        });

      devShells = forAllSystems (system:
        let pkgs = import nixpkgs { inherit system; };
        in {
          default = pkgs.mkShell {
            packages = [ pkgs.cargo pkgs.rustc pkgs.rustfmt pkgs.clippy ];
          };
        });

      nixosModules.default = { pkgs, ... }: {
        imports = [ ./nix/module.nix ];
        nixpkgs.overlays = [ overlay ];
      };
    };
}
