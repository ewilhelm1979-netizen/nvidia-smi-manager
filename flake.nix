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

      checks = forAllSystems (system:
        let pkgs = import nixpkgs { inherit system; overlays = [ overlay ]; };
        in {
          standalone-requires-module = pkgs.runCommand "nvidia-gpu-manager-standalone-guard" { } ''
            substitute ${pkgs.nvidia-gpu-manager}/bin/nvidia-gpu-manager guarded-manager \
              --replace-fail /run/current-system/sw/bin/nvidia-gpu-manager-nvidia-smi \
              /intentionally-missing/nvidia-gpu-manager-nvidia-smi
            chmod +x guarded-manager

            set +e
            output="$(./guarded-manager 2>&1)"
            status=$?
            set -e

            test "$status" -eq 78
            printf '%s\n' "$output" | grep -F 'requires the NixOS module-managed nvidia-smi compatibility wrapper'
            touch "$out"
          '';
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
