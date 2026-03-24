# Nvidia-SMI Manager für NixOS

Ein moderner CLI-basierter GPU Monitor und Manager für NixOS mit Echtzeit-Überwachung, Systemintegration und umfangreicher Funktionalität.

## Features

- 🎮 **Echtzeit GPU-Überwachung** - Live-Anzeige von GPU-Auslastung, Speicher und Temperatur
- 📊 **System-Dashboard** - Übersicht über CPU, RAM und Disk-Nutzung
- ⚙️ **Konfigurierbar** - Anpassbare Schwellwerte und Einstellungen
- 🐧 **NixOS Optimiert** - Vollständige Flake.nix Integration
- 🔔 **Alerts** - Warnungen bei kritischen Werten
- 🌓 **Watch-Modus** - Kontinuierliche Echtzeit-Überwachung
- 🎨 **Rich CLI UI** - Schöne formatierte Ausgabe mit farblichen Highlights

## Anforderungen

- Python 3.10+
- NVIDIA GPU mit installierten Treibern
- NixOS (empfohlen, läuft aber auch auf jeder Linux-Distribution)
- `nvidia-smi` Befehl verfügbar

## Installation

### Mit Flake (empfohlen für NixOS)

```bash
# Entwicklungsumgebung starten
nix flake show
nix develop

# Installation im Development Mode
pip install -e '.[dev]'
```

### Mit pip

```bash
# Installation
pip install .

# Installation mit Development-Tools
pip install '.[dev]'
```

## Verwendung

### Grundlegende Befehle

```bash
# GPU Status anzeigen
nvidia-smi-manager status

# GPU Status im Watch-Modus (mit Auto-Refresh)
nvidia-smi-manager status --watch --interval 2

# Systeminfo anzeigen
nvidia-smi-manager system

# Detaillierte GPU-Informationen (GPU 0)
nvidia-smi-manager info --gpu 0

# Konfiguration anzeigen
nvidia-smi-manager config

# Hilfe anzeigen
nvidia-smi-manager --help
```

### Watch-Modus

Starten Sie den kontinuierlichen Watch-Modus mit:

```bash
nvidia-smi-manager status --watch
```

Dies zeigt ein Live-Dashboard mit:
- GPU Index und Name
- Speichernutzung (aktuell/gesamt)
- Temperatur
- Stromverbrauch
- GPU-Auslastung
- Aktuelle Uhrzeit

Drücken Sie `Ctrl+C` zum Beenden.

## Projektstruktur

```
.
├── flake.nix                      # NixOS Flake-Konfiguration
├── flake.lock                     # Lock-Datei für reproducibility
├── pyproject.toml                 # Python-Projektdefinition
├── README.md                      # Diese Datei
├── src/
│   └── nvidia_smi_manager/
│       ├── __init__.py
│       ├── core/                  # Kern-Funktionalität
│       │   ├── gpu_monitor.py    # GPU-Monitoring
│       │   ├── system_info.py    # Systeminfo
│       │   └── config.py         # Konfiguration
│       ├── cli/                   # CLI-Interface
│       │   ├── __init__.py
│       │   └── commands.py       # CLI-Befehle
│       └── utils/
│           └── formatters.py      # Output-Formatierung
├── tests/
│   ├── __init__.py
│   └── test_gpu_monitor.py       # Unit-Tests
└── nix/
    └── default.nix               # Legacy Nix shell
```

## Entwicklung

### Setup

```bash
# In NixOS
nix develop

# Oder manuell
python -m venv venv
source venv/bin/activate
pip install -e '.[dev]'
```

### Code-Qualität

```bash
# Black (Code-Formatierung)
black src/ tests/

# isort (Import-Sortierung)
isort src/ tests/

# Flake8 (Linting)
flake8 src/ tests/

# MyPy (Type-Checking)
mypy src/

# Tests mit Coverage
pytest --cov=nvidia_smi_manager
```

### Tests ausführen

```bash
# Alle Tests
pytest

# Mit Coverage-Report
pytest --cov=nvidia_smi_manager --cov-report=html

# Spezifischen Test ausführen
pytest tests/test_gpu_monitor.py -v
```

## Konfiguration

Die Konfiguration wird in `~/.config/nvidia-smi-manager/config.json` gespeichert:

```json
{
  "refresh_interval": 2,
  "update_frequency": 5,
  "log_level": "INFO",
  "enable_alerts": true,
  "temp_threshold": 80.0,
  "power_threshold": 90.0
}
```

### Konfigurationsoptionen

| Option | Typ | Beschreibung | Standard |
|--------|-----|-------------|---------|
| `refresh_interval` | int | Refresh-Interval in Sekunden | 2 |
| `update_frequency` | int | Updates zwischen Checks | 5 |
| `log_level` | string | Log-Level (DEBUG, INFO, WARNING, ERROR) | INFO |
| `enable_alerts` | bool | Warnungen aktivieren | true |
| `temp_threshold` | float | Temperatur-Warnschwelle (°C) | 80.0 |
| `power_threshold` | float | Strom-Warnschwelle (%) | 90.0 |

## NixOS Integration

Das Projekt ist optimiert für NixOS und bietet:

- **Flake.nix** für reproducible Development Environment
- **Automatische Dependency-Resolution** via Nix
- **CUDA Toolkit Integration** in der Dev-Shell
- **Python 3.11** mit allen benötigten Packages

### NixOS Installation im System

Um das Tool systemweit zu installieren, können Sie es zu Ihrer `configuration.nix` hinzufügen:

```nix
{ config, pkgs, ... }:

{
  environment.systemPackages = with pkgs; [
    # ... andere packages
    # nvidia-smi-manager aus diesem Repository
  ];
}
```

## Fehlerbehebung

### nvidia-smi nicht gefunden

Stellen Sie sicher, dass NVIDIA-Treiber installiert sind:

```bash
# auf NixOS
nix-shell -p nvidia-docker

# oder mit nouveau-Treibern
lspci | grep -i nvidia
```

### Permission Denied

Einige Befehle benötigen erhöhte Rechte:

```bash
sudo nvidia-smi-manager status
```

### Python Module nicht gefunden

```bash
# Projekt im Development Mode installieren
pip install -e '.[dev]'

# oder PYTHONPATH setzen
export PYTHONPATH="${PYTHONPATH}:$(pwd)/src"
```

## Beiträge

Beiträge sind willkommen! Bitte:

1. Fork das Repository
2. Feature-Branch erstellen (`git checkout -b feature/AmazingFeature`)
3. Änderungen committen (`git commit -m 'Add some AmazingFeature'`)
4. Branch pushen (`git push origin feature/AmazingFeature`)
5. Pull Request öffnen

## Lizenz

Dieses Projekt ist unter der MIT-Lizenz lizenziert. Siehe [LICENSE](LICENSE) für Details.

## Kontakt

Für Fragen oder Vorschläge öffnen Sie bitte ein Issue im Repository.

## Roadmap

- [ ] GPU Process Listing (welche Prozesse nutzen GPU)
- [ ] Power Management (GPU Power Limits setzen)
- [ ] Automatische Alerts via E-Mail/Webhook
- [ ] Web-Dashboard (Webserver mit Live-Updates)
- [ ] Container Integration (Docker GPU-Monitoring)
- [ ] Prometheus Metriken-Export
- [ ] Grafana Dashboard-Templates
- [ ] Historische Daten und Logging
