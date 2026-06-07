# NVIDIA GPU Manager

Native Rust GUI for monitoring and controlling an NVIDIA GPU on NixOS. It is a
rewrite of the original `nvidia-manager.py` prototype and provides:

- live GPU, VRAM, temperature, fan, power and clock monitoring
- short history charts
- power-limit control with a graphical Polkit prompt
- driver-dependent fan control
- a reusable NixOS module

## Try it locally

```bash
nix run path:.
```

## Add it to NixOS

Add this repository as a flake input:

```nix
{
  inputs.nvidia-gpu-manager.url = "path:/home/enricow79/Projekte/Nvidia-GPU-Manager";

  outputs = { self, nixpkgs, nvidia-gpu-manager, ... }: {
    nixosConfigurations.my-host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        nvidia-gpu-manager.nixosModules.default
        ./configuration.nix
      ];
    };
  };
}
```

Enable the program in `configuration.nix`:

```nix
programs.nvidia-gpu-manager.enable = true;
```

Then rebuild:

```bash
sudo nixos-rebuild switch --flake /etc/nixos#my-host
```

The app is installed as `nvidia-gpu-manager` and also appears in desktop
application launchers. The module enables Polkit so that changing power and fan
settings can request authorization without a passwordless `sudoers` entry.
Your desktop session still needs a running Polkit authentication agent. GNOME
and KDE usually provide one automatically; minimal window managers may need one
to be started explicitly, for example `polkit-gnome-authentication-agent-1`.

## Hardware notes

Monitoring and power limits use `nvidia-smi`. Manual fan control uses
`nvidia-settings`, requires an X11 session and NVIDIA Coolbits fan control, and
is not supported by every GPU model or firmware. The module installs
`nvidia-settings`; to opt into fan control, add this to your NixOS
configuration:

```nix
services.xserver.screenSection = ''
  Option "Coolbits" "12"
'';
```

`12` enables fan control (`4`) and clock offsets (`8`). This is intentionally
not enabled by the module because it changes driver behavior. A failed control
request is reported in the status line and does not affect monitoring.
