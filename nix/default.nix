{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  name = "nvidia-smi-manager-env";
  
  buildInputs = with pkgs; [
    python311
    python311Packages.pip
    python311Packages.setuptools
    python311Packages.wheel
    python311Packages.click
    python311Packages.psutil
    python311Packages.py3nvml
    python311Packages.rich
    cuda-toolkit
    nvidia-docker
  ];

  shellHook = ''
    export PYTHONPATH="''${PYTHONPATH}:$(pwd)/src"
    echo "Nvidia-SMI Manager Development Environment"
    echo "Run: pip install -e '.[dev]' to install in development mode"
  '';
}
