# NVIDIA GPU Manager

Native Rust GUI for monitoring and controlling one or more NVIDIA GPUs on
NixOS. The application discovers every GPU reported by `nvidia-smi`, keeps
state and history per UUID, and runs hardware queries outside the egui event
loop.

## Features

- monitoring for `1..N` NVIDIA GPUs
- unambiguous GPU selector using index, model, UUID, and PCI bus identity
- compact overview of all installed GPUs
- per-GPU temperature, utilization, VRAM, fan, power, and clock history
- per-GPU power limits with interactive Polkit authentication
- UUID-scoped clock offsets when the driver exposes them
- fan control through driver-reported GPU-to-fan relationships
- resilient handling of `N/A`, malformed rows, temporary driver failures, and
  disappearing GPUs
- reusable NixOS module and desktop entry

The index shown in the UI is informational. The UUID is the stable identity
used for selection and writes. If the selected GPU disappears, selection is
cleared and all writes remain disabled until the user explicitly selects a GPU
again.

## Multi-GPU UI

The selector displays entries such as:

```text
GPU 0 - NVIDIA GeForce RTX 4090 - 01:00.0
GPU 1 - NVIDIA RTX A4000 - 09:00.0
GPU 2 - NVIDIA RTX A4000 - 0b:00.0
```

The bus ID makes identical models distinguishable. Monitoring includes an
overview of all GPUs; Power, Fan, Tuning, and Info always refer only to the
currently selected UUID.

## Try it locally

```bash
nix run path:.
```

The host must use the proprietary NVIDIA driver. Runtime commands are resolved
only from trusted NixOS system paths; the application deliberately does not
fall back to executables found through a user-controlled `PATH`.

## Add it to NixOS

Add this repository as a flake input:

```nix
{
  inputs.nvidia-gpu-manager.url =
    "github:ewilhelm1979-netizen/nvidia-smi-manager";
  inputs.nvidia-gpu-manager.inputs.nixpkgs.follows = "nixpkgs";

  outputs = { nixpkgs, nvidia-gpu-manager, ... }: {
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

Enable the module in `configuration.nix`:

```nix
programs.nvidia-gpu-manager.enable = true;
```

Then rebuild:

```bash
sudo nixos-rebuild switch --flake /etc/nixos#my-host
```

The module installs the application, enables NVIDIA Settings support, and
enables Polkit. KDE and GNOME normally provide a Polkit authentication agent;
minimal window managers need to start one separately.

No passwordless sudo rule is needed or installed.

## Fan control

Fan indices are global NV-CONTROL identifiers and are not assumed to match GPU
indices. The manager asks `nvidia-settings` for fans related to the selected
GPU with a UUID-qualified target:

```text
[GPU:<UUID>.FAN]/GPUTargetFanSpeed
```

The operation is enabled only when the driver returns at least one unique
related fan and all related fans have a common valid range. The same
UUID-qualified relationship is used for the assignment. If mapping or range
validation fails, monitoring remains available but fan control fails closed.

Fan control is not supported by every GPU, firmware, or driver. It needs an X
control display and Coolbits fan support. A Wayland desktop may expose the
compatibility X display, but this is compositor- and driver-dependent.

To opt in to fan and clock controls, add:

```nix
services.xserver.screenSection = ''
  Option "Coolbits" "12"
'';
```

`12` combines fan control (`4`) and clock offsets (`8`). The module does not
enable Coolbits automatically because these controls change driver behavior.
A new graphical login or reboot is normally required after changing it.

## Power limits

Monitoring never requires administrator rights. Before a power-limit write,
the worker performs a fresh inventory query and verifies that:

- the selected UUID is still present exactly once
- the driver still reports finite minimum and maximum limits
- the requested integer value is inside that current range

Only then does it run the fixed NixOS `pkexec` wrapper with the fixed
`nvidia-smi` executable and the UUID as a separate argument. There is no shell
and no string is interpreted as shell syntax.

The current implementation retains generic interactive Polkit authentication
for `nvidia-smi`. A narrower policy would require a dedicated privileged helper
that accepts only a validated UUID and power limit. No setuid helper or broad
passwordless policy is included.

## Tuning

Clock capabilities are probed independently for each GPU. `nvidia-settings`
must confirm that its UUID target matches the selected `nvidia-smi` UUID.
Offsets are then checked against freshly queried driver ranges before each
assignment.

Clock changes can destabilize the desktop or compute workloads. Start with
small offsets and test the affected GPU. The driver may expose different
capabilities for GeForce and professional cards.

## Security model

Trust boundaries and protections:

- `nvidia-smi` and `nvidia-settings` output is treated as untrusted input.
- Mandatory identity fields are strictly validated.
- Unsupported and malformed metrics become unavailable values, never numeric
  zeroes used for safety decisions.
- UUIDs are restricted before they can enter NVIDIA target syntax.
- All processes use `std::process::Command` with separate arguments; no shell
  is used.
- Executables are accepted only from fixed NixOS paths.
- Every write revalidates identity, capability, mapping, and range.
- A single worker thread serializes driver access and keeps the UI responsive;
  refreshes cannot create an unbounded number of threads.
- A failure affecting one GPU does not stop valid GPUs from being monitored.

Residual limitations:

- `pkexec nvidia-smi` is broader than a purpose-built privileged helper, though
  it remains interactive and uses fixed executable paths and validated args.
- `nvidia-settings` depends on the NVIDIA X control interface even when the
  desktop itself runs on Wayland.
- The driver does not provide atomic multi-attribute clock updates; a second
  offset assignment can fail after the first one succeeded.
- Hardware writes are not exercised by automated tests because they require
  explicit authentication and can affect running workloads.

## Development

```bash
nix develop
cargo fmt --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
nix flake check
nix build
```

No additional Rust dependency was introduced for multi-GPU support. Parsing,
worker communication, and process execution use the standard library.
