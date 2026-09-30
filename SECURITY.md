# Security model

This document describes the security boundaries of NVIDIA GPU Manager 0.2.0.
It is not a claim that the application or the NVIDIA driver is fully secure.

## Trust boundaries

The application treats `nvidia-smi`, `nvidia-settings`, NV-CONTROL, GPU UUIDs,
and all driver output as external input. Parsed rows, numbers, identifiers,
capabilities, target relationships, and ranges may be absent or malformed.

The manager applies these controls:

- NVIDIA commands are started directly with `std::process::Command` and
  separate arguments. It does not use a shell, `sh -c`, or `bash -c`.
- Executables are resolved only at fixed trusted NixOS paths. There is no
  fallback to a user-controlled `PATH`.
- GPU UUIDs are syntactically validated before use in command arguments or
  NV-CONTROL target expressions.
- Every write uses a fresh GPU inventory and identity check.
- Power, manual fan-speed, and tuning writes use the relevant freshly probed
  capabilities, mappings, and ranges.
- Fan writes require a unique GPU relationship and concrete fan target IDs.
- Missing, ambiguous, stale, or malformed identity and mapping data fails
  closed.
- If the selected GPU disappears, writes remain blocked until the user
  explicitly selects a GPU again.
- A single worker serializes hardware access and keeps commands outside the UI
  event loop.
- Monitoring is unprivileged and continues when optional controls are absent.

## Privilege boundary

Power-limit changes use the fixed NixOS `pkexec` wrapper to invoke a fixed
`nvidia-smi` path after UUID and range validation. Authentication remains
interactive. This has a broader authorization surface than a purpose-built
minimal helper, but avoids passwordless sudo and user-selected commands.

The module does not install `NOPASSWD` sudo rules, a setuid helper, or a broad
custom Polkit rule. Fan and clock controls use the current user's NV-CONTROL
session and do not invoke Polkit.

## Residual risks

- Generic `pkexec nvidia-smi` is broader than a dedicated power-limit helper.
- Fan and tuning controls depend on the proprietary driver and a working X11
  or XWayland/NV-CONTROL interface.
- Multiple clock attributes cannot be changed atomically; a later assignment
  can fail after an earlier one succeeded.
- Driver behavior and reported capability ranges remain outside the
  application's trust boundary.
- Automated tests do not perform hardware writes. Power, fan, and tuning
  changes require separate, explicit human authorization and observation.
