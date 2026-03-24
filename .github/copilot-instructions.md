# Nvidia-SMI Manager für NixOS - Projektanleitung

Ein CLI-basierter GPU Monitor und Manager für NixOS mit Echtzeit-Überwachung und Systemintegration.

## Projektdetails
- **Sprache**: Python 3.10+
- **Framework**: Click (CLI), psutil, py3nvml
- **NixOS Integration**: Flake.nix für reproducible Umgebung
- **Zielgruppe**: NixOS-Nutzer mit NVIDIA GPUs

## Setup-Fortschritt

- [x] Copilot-instructions.md erstellt
- [ ] Projektstruktur erstellen
- [ ] Core Manager implementieren
- [ ] NixOS Flake konfigurieren  
- [ ] CLI Interface aufbauen
- [ ] Tests & Dependencies
- [ ] Dokumentation finalisieren

## Projektstruktur
```
.
├── flake.nix
├── flake.lock
├── pyproject.toml
├── README.md
├── src/
│   └── nvidia_smi_manager/
│       ├── __init__.py
│       ├── core/
│       │   ├── gpu_monitor.py
│       │   ├── system_info.py
│       │   └── config.py
│       ├── cli/
│       │   ├── __init__.py
│       │   └── commands.py
│       └── utils/
│           └── formatters.py
├── tests/
│   ├── __init__.py
│   └── test_gpu_monitor.py
└── nix/
    └── default.nix
```
