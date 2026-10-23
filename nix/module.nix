{ config, lib, pkgs, ... }:

let
  cfg = config.programs.nvidia-gpu-manager;
  nvidiaPackage = config.hardware.nvidia.package;
  nvidiaBin = nvidiaPackage.bin or nvidiaPackage;
  nvidiaVersion = lib.getVersion nvidiaPackage;

  # patchelf 0.19.2 can create overlapping ELF LOAD segments when changing the
  # interpreter and RPATH together. Re-patch the pristine NVIDIA binary in two
  # ordered invocations until the nixpkgs builder no longer needs this repair.
  repatchedNvidiaSmi = pkgs.runCommand "nvidia-smi-${nvidiaVersion}-elf-compat" {
    nativeBuildInputs = [ pkgs.patchelf ];
  } ''
    broken=${nvidiaBin}/bin/nvidia-smi
    pristine=${nvidiaBin}/origBin/nvidia-smi

    test -x "$broken"
    test -x "$pristine"

    install -Dm755 "$pristine" "$out/bin/nvidia-smi"
    rpath="$(patchelf --print-rpath "$broken")"
    interpreter="$(patchelf --print-interpreter "$broken")"
    patchelf --set-rpath "$rpath" "$out/bin/nvidia-smi"
    patchelf --set-interpreter "$interpreter" "$out/bin/nvidia-smi"
  '';

  nvidiaSmiWrapper = pkgs.writeShellScriptBin "nvidia-gpu-manager-nvidia-smi" ''
    set -eu

    loaded_version=
    if ! IFS= read -r loaded_version < /sys/module/nvidia/version; then
      printf '%s\n' 'nvidia-smi blocked: the loaded NVIDIA driver version is unavailable.' >&2
      exit 78
    fi
    if [ "$loaded_version" != ${lib.escapeShellArg nvidiaVersion} ]; then
      printf '%s\n' "nvidia-smi blocked: loaded driver $loaded_version does not match packaged driver ${nvidiaVersion}. Reboot into the current NixOS generation." >&2
      exit 78
    fi

    exec ${repatchedNvidiaSmi}/bin/nvidia-smi "$@"
  '';
in
{
  options.programs.nvidia-gpu-manager = {
    enable = lib.mkEnableOption "NVIDIA GPU Manager";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.nvidia-gpu-manager;
      defaultText = lib.literalExpression "pkgs.nvidia-gpu-manager";
      description = "The nvidia-gpu-manager package to install.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package nvidiaSmiWrapper ];
    hardware.nvidia.nvidiaSettings = true;
    security.polkit.enable = true;
  };
}
