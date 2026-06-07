{ config, lib, pkgs, ... }:

let
  cfg = config.programs.nvidia-gpu-manager;
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
    environment.systemPackages = [ cfg.package ];
    hardware.nvidia.nvidiaSettings = true;
    security.polkit.enable = true;
  };
}
