# Changelog

## 0.2.1 - 2026-10-23

### Fixed

- Restored inventory and monitoring when the nixpkgs `patchelf` 0.19.2
  regression makes the packaged NVIDIA utilities exit with SIGSEGV.
- Re-patched the pristine, host-matched `nvidia-smi` binary with separate RPATH
  and interpreter operations to avoid overlapping ELF load segments.
- Standalone package launches now fail closed with exit status 78 until the
  NixOS module has installed its host-matched compatibility wrapper.
- A running manager also fails closed if that module-managed path disappears;
  it never falls back to an unqualified system NVIDIA binary.

### Security

- The manager prefers a dedicated root-controlled NixOS system path instead of
  accepting a user-profile or `PATH` fallback.
- The compatibility wrapper verifies the loaded NVIDIA kernel driver version
  against the packaged driver before monitoring or privileged power changes.

## 0.2.0 - 2026-09-30

### Added

- Multi-GPU support for `1..N` NVIDIA GPUs
- UUID-based GPU selection with model, index, and PCI-bus identity
- All-GPU monitoring overview
- Per-GPU histories and power, fan, and tuning state
- Serialized hardware worker
- Scrollable tab content with a fixed status bar

### Security

- Fixed executable resolution without user-controlled `PATH` fallbacks
- Strict UUID validation
- Fresh identity, range, capability, and fan-mapping checks before writes
- Fail-closed hotplug handling
- Concrete `[fan:<id>]` write targets after relationship discovery
- Automatic-mode rollback attempt after a failed fan write

### Fixed

- Static GPU 0 assumptions
- Static fan 0 assumptions
- `N/A` values interpreted as real zeroes
- Display-qualified NVIDIA target parsing
- Permanent capability disablement after transient probe errors

### Known limitations

- Power limits use generic interactive `pkexec nvidia-smi`
- Fan and tuning controls depend on an available NV-CONTROL interface
- Clock changes are not atomic
- Hardware write tests require explicit human authorization
