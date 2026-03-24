{
  description = "NV-SMI Manager for NixOS";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        pythonEnv = pkgs.python312.withPackages (ps: with ps; [
          click
          psutil
          py3nvml
          rich
          pytest
          pytest-cov
          black
          isort
          flake8
          mypy
        ]);
      in
      {
        devShells.default = pkgs.mkShell {
          name = "nv-smi-manager-dev";
          buildInputs = [
            pythonEnv
          ];

          shellHook = ''
            export PYTHONPATH="${self}/src:$PYTHONPATH"
            echo "❯ NV-SMI Manager Development Environment (Python 3.12)"
            echo "  Run: nv-smi-manager status"
            echo "  Test: pytest tests/ -v"
            echo "  Format: black src/ tests/ --line-length=100"
            echo ""
            echo "  Project: https://github.com/ewilhelm1979-netizen/nvidia-smi-manager"
          '';
        };
      }
    );
}
