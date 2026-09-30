# NVIDIA GPU Manager

Native Rust GUI for monitoring and controlling one or more NVIDIA GPUs on
NixOS. The application discovers every GPU reported by `nvidia-smi`, keeps
state and history per UUID, and runs hardware queries outside the egui event
loop. This document describes version `0.2.0`.

## Features

- monitoring for `1..N` NVIDIA GPUs
- unambiguous GPU selector using index, model, UUID, and PCI bus identity
- compact overview of all installed GPUs
- per-GPU temperature, utilization, fan, and power history
- per-GPU VRAM and clock monitoring
- per-GPU power limits with interactive Polkit authentication
- UUID-scoped clock offsets when the driver exposes them
- fan control through driver-reported GPU-to-fan relationships
- resilient handling of `N/A`, malformed rows, temporary driver failures, and
  disappearing GPUs
- reusable NixOS module and desktop entry

The index shown in the UI is informational. The UUID is the stable identity
used for selection and writes. If the selected GPU disappears, selection is
cleared and all writes remain disabled until the user explicitly selects a GPU
again. A fresh inventory and identity check is performed before every write, so
a stored index is never trusted after hotplug, a driver reset, or GPU
renumbering.

## Multi-GPU UI

The target host may contain a GeForce RTX 4090 with 24 GB and two RTX A4000
cards with 16 GB each. Capacity is part of that hardware description, not the
selector label. The selector displays the driver-reported name and PCI bus,
for example:

```text
GPU 0 - NVIDIA GeForce RTX 4090 - 01:00.0
GPU 1 - NVIDIA RTX A4000 - 09:00.0
GPU 2 - NVIDIA RTX A4000 - 0b:00.0
```

The bus ID makes identical models distinguishable. Monitoring includes an
overview of all GPUs; Power, Fan, Tuning, and Info always refer only to the
currently selected UUID. Histories, desired power limit, fan state, tuning
values, and probed capabilities are maintained separately for each GPU. The UI
is not limited to the three devices in this example; it supports `1..N` GPUs.

## Try it locally

```bash
nix run path:.
```

The host must use the proprietary NVIDIA driver. Runtime commands are resolved
only from trusted NixOS system paths; the application deliberately does not
fall back to executables found through a user-controlled `PATH`.

## Add it to NixOS

### Normal installation

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

### Reproducibly pinned production installation

Production systems should pin an exact commit that has passed their local
qualification instead of following a moving branch:

```nix
inputs.nvidia-gpu-manager.url =
  "github:ewilhelm1979-netizen/nvidia-smi-manager/<COMMIT>";
inputs.nvidia-gpu-manager.inputs.nixpkgs.follows = "nixpkgs";
```

Update that commit deliberately and refresh only this input in the consumer's
lock file.

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
GPU with the NV-CONTROL target qualifier `[GPU:<UUID>.FAN]`. With the queried
attribute, relationship discovery uses:

```text
[GPU:<UUID>.FAN]/GPUTargetFanSpeed
```

This qualifier is used only for discovery. The operation is enabled only when
the driver returns at least one unique related fan and all related fans have a
common valid range. Writes are sent exclusively to each verified concrete
target as `[fan:<id>]/GPUTargetFanSpeed`. Missing, malformed, duplicate, or
ambiguous mappings fail closed.

On one read-only qualification host, the relationship query returned fans 0
and 1 for an RTX 4090 and fan 2 for an RTX A4000. This is a hardware-specific
example, not a general NVIDIA fan numbering rule.

Restoring automatic control remains available because it writes only the
selected UUID's `GPUFanControlState` and does not depend on a speed mapping. If
one concrete fan write fails after manual mode was enabled, the manager tries
to roll that GPU back to automatic control. Automated tests do not perform real
fan writes.

## Capability handling

Capability probes distinguish confirmed unsupported attributes from temporary
driver, display-server, NV-CONTROL, and parsing failures:

- `Unsupported` means the driver explicitly does not expose the capability.
- `Retryable` means the probe could not establish a reliable answer yet.

Retryable capabilities are probed again while the application is running, so a
temporary XWayland or driver failure does not permanently disable a control.
`nvidia-settings` may qualify returned targets with a display prefix, for
example `hostname:0[fan:0]` or `hostname:0[gpu:0]`; both forms are supported by
the parser.

## Display server requirements

Monitoring uses `nvidia-smi` and does not require X11. Fan control and clock
offsets use NV-CONTROL through `nvidia-settings`, which requires a working X11
control display or an XWayland/NV-CONTROL bridge. A Wayland session alone does
not guarantee that these controls are available.

Fan control is not supported by every GPU, firmware, or driver. It needs an X
control display and Coolbits fan support. A Wayland desktop may expose the
compatibility X display, but this is compositor- and driver-dependent.

To opt in to fan and clock controls, add:

```nix
services.xserver.screenSection = ''
  Option "Coolbits" "12"
'';
```

The Coolbits mask uses `4` for fan control and `8` for clock offsets; `12` is
their combination. The module does not enable Coolbits automatically because
this is an explicit host policy and changes driver behavior. A new graphical
login or reboot is normally required after changing it.

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

The detailed trust boundaries and residual risks are documented in
[`SECURITY.md`](SECURITY.md). The main protections are:

- `nvidia-smi` and `nvidia-settings` output is treated as untrusted input.
- Mandatory identity fields are strictly validated.
- Unsupported and malformed metrics become unavailable values, never numeric
  zeroes used for safety decisions.
- UUIDs are restricted before they can enter NVIDIA target syntax.
- All processes use `std::process::Command` with separate arguments; no shell,
  `sh -c`, or `bash -c` is used.
- Executables are accepted only from fixed NixOS paths, with no user-controlled
  `PATH` fallback.
- Every write performs a fresh identity check plus the capability, mapping, and
  range checks relevant to that operation.
- A single worker thread serializes driver access and keeps the UI responsive;
  refreshes cannot create an unbounded number of threads.
- A failure affecting one GPU does not stop valid GPUs from being monitored.
- Monitoring is unprivileged. Only power-limit changes cross the Polkit
  boundary.

Residual limitations:

- `pkexec nvidia-smi` is broader than a purpose-built privileged helper, though
  it remains interactive and uses fixed executable paths and validated args.
- `nvidia-settings` depends on the NVIDIA X control interface even when the
  desktop itself runs on Wayland.
- The driver does not provide atomic multi-attribute clock updates; a second
  offset assignment can fail after the first one succeeded.
- Hardware writes are not exercised by automated tests because they require
  explicit authentication and can affect running workloads.

No passwordless sudo rule or setuid helper is installed.

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
