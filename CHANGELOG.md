# Changelog

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
