{
  description = "Nvidia-SMI Manager for NixOS";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        python = pkgs.python311;
        pythonPackages = python.pkgs;
      in
      {
        packages.default = python.pkgs.buildPythonPackage {
          pname = "nvidia-smi-manager";
          version = "0.1.0";
          src = ./.;

          propagatedBuildInputs = with python.pkgs; [
            click
            psutil
            py3nvml
            rich
          ];

          nativeBuildInputs = with python.pkgs; [
            setuptools
            wheel
          ];

          doCheck = true;
          checkInputs = with python.pkgs; [
            pytest
            pytest-cov
          ];
        };

        devShells.default = pkgs.mkShell {
          name = "nvidia-smi-manager-dev";
          buildInputs = with pkgs; [
            python311
            python311Packages.pip
            python311Packages.black
            python311Packages.isort
            python311Packages.flake8
            python311Packages.mypy
            python311Packages.pytest
            python311Packages.pytest-cov
            cuda-toolkit
          ];

          shellHook = ''
            export PYTHONPATH="${self}/src:$PYTHONPATH"
            echo "Nvidia-SMI Manager Development Environment"
            echo "Run: pip install -e '.[dev]' to install in development mode"
          '';
        };
      }
    );
}
