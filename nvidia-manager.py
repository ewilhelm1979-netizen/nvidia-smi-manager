#!/usr/bin/env python3
"""
╔══════════════════════════════════════════╗
║       NVIDIA GPU Manager – NixOS        ║
║  Live Monitoring · Power · Fan Control  ║
╚══════════════════════════════════════════╝
"""

import sys
import subprocess
import os
import signal
from collections import deque

from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QLabel, QSlider, QPushButton, QTabWidget, QProgressBar,
    QSystemTrayIcon, QMenu, QAction, QGroupBox, QGridLayout,
    QFrame, QSizePolicy, QScrollArea
)
from PyQt6.QtCore import QTimer, Qt, QThread, pyqtSignal, QSize
from PyQt6.QtGui import (
    QIcon, QPixmap, QColor, QPainter, QFont, QPen, QBrush,
    QLinearGradient, QPainterPath, QFontDatabase
)


# ── Farben ──────────────────────────────────────────────────────────────────
C_BG        = "#0d0f14"
C_SURFACE   = "#141720"
C_CARD      = "#1a1e2a"
C_BORDER    = "#252a3a"
C_ACCENT    = "#76b900"   # NVIDIA Grün
C_ACCENT2   = "#00b4d8"   # Cyan für Memory
C_WARN      = "#ff9f1c"
C_DANGER    = "#e63946"
C_TEXT      = "#e8eaf0"
C_MUTED     = "#6b7280"


# ── nvidia-smi Helfer ────────────────────────────────────────────────────────
def run_smi(*args) -> str:
    try:
        result = subprocess.run(
            ["nvidia-smi"] + list(args),
            capture_output=True, text=True, timeout=3
        )
        return result.stdout.strip()
    except Exception:
        return ""


def get_gpu_stats() -> dict:
    fields = [
        "name",
        "temperature.gpu",
        "utilization.gpu",
        "utilization.memory",
        "memory.used",
        "memory.total",
        "fan.speed",
        "power.draw",
        "power.limit",
        "power.min_limit",
        "power.max_limit",
        "clocks.gr",
        "clocks.mem",
        "clocks.max.gr",
        "clocks.max.mem",
        "pcie.link.gen.current",
        "pcie.link.width.current",
    ]
    query = ",".join(fields)
    raw = run_smi(
        f"--query-gpu={query}",
        "--format=csv,noheader,nounits"
    )
    if not raw:
        return {}

    values = [v.strip() for v in raw.split(",")]
    keys = [f.split(".")[0] if f.count(".") > 1 else f for f in fields]

    def safe_float(v):
        try:
            return float(v)
        except Exception:
            return 0.0

    return {
        "name":         values[0],
        "temp":         safe_float(values[1]),
        "gpu_util":     safe_float(values[2]),
        "mem_util":     safe_float(values[3]),
        "mem_used":     safe_float(values[4]),
        "mem_total":    safe_float(values[5]),
        "fan":          safe_float(values[6]),
        "power_draw":   safe_float(values[7]),
        "power_limit":  safe_float(values[8]),
        "power_min":    safe_float(values[9]),
        "power_max":    safe_float(values[10]),
        "clk_gpu":      safe_float(values[11]),
        "clk_mem":      safe_float(values[12]),
        "clk_gpu_max":  safe_float(values[13]),
        "clk_mem_max":  safe_float(values[14]),
        "pcie_gen":     values[15],
        "pcie_width":   values[16],
    }


def set_power_limit(watts: int) -> bool:
    result = subprocess.run(
        ["sudo", "nvidia-smi", "-i", "0", f"-pl", str(watts)],
        capture_output=True, text=True
    )
    return result.returncode == 0


def set_fan_speed(pct: int) -> bool:
    result = subprocess.run(
        ["sudo", "nvidia-smi", "-i", "0", "--fan-speed-percent", str(pct)],
        capture_output=True, text=True
    )
    return result.returncode == 0


def set_fan_auto() -> bool:
    result = subprocess.run(
        ["sudo", "nvidia-smi", "-i", "0", "--fan-speed-percent", "0"],
        capture_output=True, text=True
    )
    return result.returncode == 0


# ── Mini-Chart Widget ─────────────────────────────────────────────────────────
class SparklineChart(QWidget):
    def __init__(self, color=C_ACCENT, max_val=100, parent=None):
        super().__init__(parent)
        self.color = QColor(color)
        self.max_val = max_val
        self.data = deque([0.0] * 60, maxlen=60)
        self.setMinimumHeight(60)
        self.setMinimumWidth(200)

    def push(self, value: float):
        self.data.append(value)
        self.update()

    def paintEvent(self, event):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        w = self.width()
        h = self.height()

        # Hintergrund
        p.fillRect(0, 0, w, h, QColor(C_CARD))

        if len(self.data) < 2:
            return

        pts = list(self.data)
        n = len(pts)
        step = w / (n - 1)

        # Gradient fill
        path = QPainterPath()
        path.moveTo(0, h)
        for i, v in enumerate(pts):
            x = i * step
            y = h - (v / self.max_val) * (h - 4)
            if i == 0:
                path.lineTo(x, y)
            else:
                path.lineTo(x, y)
        path.lineTo(w, h)
        path.closeSubpath()

        grad = QLinearGradient(0, 0, 0, h)
        c = QColor(self.color)
        c.setAlpha(80)
        grad.setColorAt(0, c)
        c2 = QColor(self.color)
        c2.setAlpha(10)
        grad.setColorAt(1, c2)
        p.fillPath(path, QBrush(grad))

        # Linie
        pen = QPen(self.color, 2)
        p.setPen(pen)
        line_path = QPainterPath()
        for i, v in enumerate(pts):
            x = i * step
            y = h - (v / self.max_val) * (h - 4)
            if i == 0:
                line_path.moveTo(x, y)
            else:
                line_path.lineTo(x, y)
        p.drawPath(line_path)

        p.end()


# ── Stat-Karte ────────────────────────────────────────────────────────────────
class StatCard(QWidget):
    def __init__(self, title: str, unit: str, color=C_ACCENT, max_val=100, parent=None):
        super().__init__(parent)
        self.unit = unit
        self.setStyleSheet(f"""
            QWidget {{
                background: {C_CARD};
                border: 1px solid {C_BORDER};
                border-radius: 10px;
            }}
        """)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(14, 12, 14, 12)
        layout.setSpacing(6)

        # Titel
        lbl_title = QLabel(title)
        lbl_title.setStyleSheet(f"color: {C_MUTED}; font-size: 11px; font-weight: 600; letter-spacing: 1px; border: none; background: transparent;")
        layout.addWidget(lbl_title)

        # Wert
        self.lbl_value = QLabel("—")
        self.lbl_value.setStyleSheet(f"color: {color}; font-size: 28px; font-weight: 700; border: none; background: transparent;")
        layout.addWidget(self.lbl_value)

        # Chart
        self.chart = SparklineChart(color=color, max_val=max_val)
        layout.addWidget(self.chart)

    def update_value(self, value: float):
        self.lbl_value.setText(f"{value:.0f} {self.unit}")
        self.chart.push(value)


# ── Schieberegler-Karte ───────────────────────────────────────────────────────
class ControlCard(QGroupBox):
    def __init__(self, title: str, parent=None):
        super().__init__(title, parent)
        self.setStyleSheet(f"""
            QGroupBox {{
                background: {C_CARD};
                border: 1px solid {C_BORDER};
                border-radius: 10px;
                color: {C_TEXT};
                font-size: 13px;
                font-weight: 600;
                margin-top: 12px;
                padding-top: 8px;
            }}
            QGroupBox::title {{
                subcontrol-origin: margin;
                left: 12px;
                padding: 0 6px;
                color: {C_MUTED};
                font-size: 11px;
                letter-spacing: 1px;
            }}
        """)


# ── Tray Icon (grünes NVIDIA-Quadrat) ────────────────────────────────────────
def make_tray_icon(temp: float = 0) -> QIcon:
    px = QPixmap(22, 22)
    px.fill(Qt.GlobalColor.transparent)
    p = QPainter(px)
    p.setRenderHint(QPainter.RenderHint.Antialiasing)

    if temp < 60:
        color = QColor(C_ACCENT)
    elif temp < 80:
        color = QColor(C_WARN)
    else:
        color = QColor(C_DANGER)

    p.setBrush(QBrush(color))
    p.setPen(Qt.PenStyle.NoPen)
    p.drawRoundedRect(2, 2, 18, 18, 4, 4)

    p.setPen(QPen(QColor("white")))
    font = QFont()
    font.setPixelSize(8)
    font.setBold(True)
    p.setFont(font)
    p.drawText(px.rect(), Qt.AlignmentFlag.AlignCenter, "GPU")
    p.end()
    return QIcon(px)


# ── Haupt-Fenster ─────────────────────────────────────────────────────────────
class NvidiaManager(QMainWindow):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("NVIDIA GPU Manager")
        self.setMinimumSize(820, 620)
        self.fan_manual = False
        self._init_style()
        self._init_ui()
        self._init_tray()
        self._init_timer()
        self._refresh()

    # ── Style ──────────────────────────────────────────────────────────────
    def _init_style(self):
        self.setStyleSheet(f"""
            QMainWindow, QWidget {{
                background-color: {C_BG};
                color: {C_TEXT};
                font-family: 'Segoe UI', 'Noto Sans', sans-serif;
                font-size: 13px;
            }}
            QTabWidget::pane {{
                border: 1px solid {C_BORDER};
                border-radius: 8px;
                background: {C_BG};
            }}
            QTabBar::tab {{
                background: {C_SURFACE};
                color: {C_MUTED};
                padding: 10px 20px;
                border: none;
                font-size: 12px;
                font-weight: 600;
                letter-spacing: 0.5px;
            }}
            QTabBar::tab:selected {{
                background: {C_CARD};
                color: {C_ACCENT};
                border-bottom: 2px solid {C_ACCENT};
            }}
            QTabBar::tab:hover {{
                color: {C_TEXT};
            }}
            QSlider::groove:horizontal {{
                background: {C_BORDER};
                height: 6px;
                border-radius: 3px;
            }}
            QSlider::handle:horizontal {{
                background: {C_ACCENT};
                width: 16px;
                height: 16px;
                margin: -5px 0;
                border-radius: 8px;
            }}
            QSlider::sub-page:horizontal {{
                background: {C_ACCENT};
                border-radius: 3px;
            }}
            QPushButton {{
                background: {C_SURFACE};
                color: {C_TEXT};
                border: 1px solid {C_BORDER};
                border-radius: 6px;
                padding: 8px 18px;
                font-weight: 600;
            }}
            QPushButton:hover {{
                background: {C_CARD};
                border-color: {C_ACCENT};
                color: {C_ACCENT};
            }}
            QPushButton:pressed {{
                background: {C_ACCENT};
                color: {C_BG};
            }}
            QLabel {{ background: transparent; }}
            QProgressBar {{
                background: {C_BORDER};
                border-radius: 4px;
                text-align: center;
                color: {C_TEXT};
                font-size: 11px;
                font-weight: 600;
                height: 18px;
            }}
            QProgressBar::chunk {{
                border-radius: 4px;
                background: qlineargradient(x1:0, y1:0, x2:1, y2:0,
                    stop:0 {C_ACCENT}, stop:1 #9fd93a);
            }}
            QScrollArea {{ border: none; }}
        """)

    # ── UI aufbauen ────────────────────────────────────────────────────────
    def _init_ui(self):
        central = QWidget()
        self.setCentralWidget(central)
        root = QVBoxLayout(central)
        root.setContentsMargins(16, 16, 16, 16)
        root.setSpacing(12)

        # Header
        header = QHBoxLayout()
        self.lbl_gpu_name = QLabel("NVIDIA GPU Manager")
        self.lbl_gpu_name.setStyleSheet(f"color: {C_ACCENT}; font-size: 18px; font-weight: 700;")
        header.addWidget(self.lbl_gpu_name)
        header.addStretch()
        self.lbl_driver = QLabel("")
        self.lbl_driver.setStyleSheet(f"color: {C_MUTED}; font-size: 11px;")
        header.addWidget(self.lbl_driver)
        root.addLayout(header)

        # Separator
        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.HLine)
        sep.setStyleSheet(f"color: {C_BORDER};")
        root.addWidget(sep)

        # Tabs
        tabs = QTabWidget()
        root.addWidget(tabs)

        tabs.addTab(self._build_monitor_tab(), "⬛  MONITORING")
        tabs.addTab(self._build_power_tab(),   "⚡  POWER")
        tabs.addTab(self._build_fan_tab(),     "🌀  LÜFTER")
        tabs.addTab(self._build_info_tab(),    "ℹ  INFO")

        # Status-Leiste
        self.status_bar = QLabel("Bereit")
        self.status_bar.setStyleSheet(f"color: {C_MUTED}; font-size: 11px; padding: 4px 0;")
        root.addWidget(self.status_bar)

    # ── Monitoring Tab ─────────────────────────────────────────────────────
    def _build_monitor_tab(self) -> QWidget:
        w = QWidget()
        layout = QVBoxLayout(w)
        layout.setContentsMargins(12, 12, 12, 12)
        layout.setSpacing(12)

        # Stat-Karten Reihe 1
        row1 = QHBoxLayout()
        self.card_temp    = StatCard("TEMPERATUR",    "°C",  "#e63946", max_val=100)
        self.card_gpu     = StatCard("GPU AUSLASTUNG", "%",  C_ACCENT,  max_val=100)
        self.card_power   = StatCard("LEISTUNG",      "W",  C_WARN,    max_val=500)
        self.card_fan     = StatCard("LÜFTER",        "%",  C_ACCENT2, max_val=100)
        for c in [self.card_temp, self.card_gpu, self.card_power, self.card_fan]:
            row1.addWidget(c)
        layout.addLayout(row1)

        # Progressbars
        bars = ControlCard("SPEICHER & AUSLASTUNG")
        bars_layout = QGridLayout(bars)
        bars_layout.setSpacing(10)

        bars_layout.addWidget(QLabel("GPU:"),    0, 0)
        self.bar_gpu = QProgressBar()
        bars_layout.addWidget(self.bar_gpu,      0, 1)
        self.lbl_gpu_pct = QLabel("0 %")
        self.lbl_gpu_pct.setFixedWidth(50)
        bars_layout.addWidget(self.lbl_gpu_pct,  0, 2)

        bars_layout.addWidget(QLabel("VRAM:"),   1, 0)
        self.bar_mem = QProgressBar()
        self.bar_mem.setStyleSheet(self.bar_mem.styleSheet().replace(C_ACCENT, C_ACCENT2))
        bars_layout.addWidget(self.bar_mem,      1, 1)
        self.lbl_mem_val = QLabel("0 / 0 MiB")
        self.lbl_mem_val.setFixedWidth(120)
        bars_layout.addWidget(self.lbl_mem_val,  1, 2)

        bars_layout.addWidget(QLabel("Temp:"),   2, 0)
        self.bar_temp = QProgressBar()
        bars_layout.addWidget(self.bar_temp,     2, 1)
        self.lbl_temp_val = QLabel("0 °C")
        self.lbl_temp_val.setFixedWidth(50)
        bars_layout.addWidget(self.lbl_temp_val, 2, 2)

        layout.addWidget(bars)

        # Takt-Info
        clk_group = ControlCard("TAKTFREQUENZEN")
        clk_layout = QGridLayout(clk_group)
        clk_layout.setSpacing(10)

        for col, txt in enumerate(["", "Aktuell", "Maximum"]):
            lbl = QLabel(txt)
            lbl.setStyleSheet(f"color: {C_MUTED}; font-size: 11px; font-weight: 600;")
            clk_layout.addWidget(lbl, 0, col)

        clk_layout.addWidget(QLabel("GPU Core:"), 1, 0)
        self.lbl_clk_gpu = QLabel("— MHz")
        self.lbl_clk_gpu_max = QLabel("— MHz")
        self.lbl_clk_gpu.setStyleSheet(f"color: {C_ACCENT}; font-weight: 600;")
        clk_layout.addWidget(self.lbl_clk_gpu, 1, 1)
        clk_layout.addWidget(self.lbl_clk_gpu_max, 1, 2)

        clk_layout.addWidget(QLabel("Speicher:"), 2, 0)
        self.lbl_clk_mem = QLabel("— MHz")
        self.lbl_clk_mem_max = QLabel("— MHz")
        self.lbl_clk_mem.setStyleSheet(f"color: {C_ACCENT2}; font-weight: 600;")
        clk_layout.addWidget(self.lbl_clk_mem, 2, 1)
        clk_layout.addWidget(self.lbl_clk_mem_max, 2, 2)

        layout.addWidget(clk_group)
        layout.addStretch()
        return w

    # ── Power Tab ─────────────────────────────────────────────────────────
    def _build_power_tab(self) -> QWidget:
        w = QWidget()
        layout = QVBoxLayout(w)
        layout.setContentsMargins(12, 12, 12, 12)
        layout.setSpacing(16)

        # Aktueller Verbrauch
        info = ControlCard("AKTUELLER VERBRAUCH")
        info_layout = QGridLayout(info)
        info_layout.setSpacing(10)

        info_layout.addWidget(QLabel("Leistungsaufnahme:"), 0, 0)
        self.lbl_power_draw = QLabel("— W")
        self.lbl_power_draw.setStyleSheet(f"color: {C_WARN}; font-size: 20px; font-weight: 700;")
        info_layout.addWidget(self.lbl_power_draw, 0, 1)

        info_layout.addWidget(QLabel("Aktuelles Limit:"), 1, 0)
        self.lbl_power_limit_cur = QLabel("— W")
        self.lbl_power_limit_cur.setStyleSheet(f"color: {C_TEXT}; font-size: 16px; font-weight: 600;")
        info_layout.addWidget(self.lbl_power_limit_cur, 1, 1)

        info_layout.addWidget(QLabel("Min / Max:"), 2, 0)
        self.lbl_power_range = QLabel("— W / — W")
        self.lbl_power_range.setStyleSheet(f"color: {C_MUTED};")
        info_layout.addWidget(self.lbl_power_range, 2, 1)
        layout.addWidget(info)

        # Power Limit Slider
        ctrl = ControlCard("POWER LIMIT SETZEN  (benötigt sudo)")
        ctrl_layout = QVBoxLayout(ctrl)
        ctrl_layout.setSpacing(10)

        self.lbl_power_slider_val = QLabel("250 W")
        self.lbl_power_slider_val.setStyleSheet(
            f"color: {C_WARN}; font-size: 22px; font-weight: 700; qproperty-alignment: AlignCenter;"
        )
        ctrl_layout.addWidget(self.lbl_power_slider_val)

        self.slider_power = QSlider(Qt.Orientation.Horizontal)
        self.slider_power.setRange(100, 600)
        self.slider_power.setValue(250)
        self.slider_power.valueChanged.connect(
            lambda v: self.lbl_power_slider_val.setText(f"{v} W")
        )
        ctrl_layout.addWidget(self.slider_power)

        range_row = QHBoxLayout()
        self.lbl_power_min_lbl = QLabel("100 W")
        self.lbl_power_min_lbl.setStyleSheet(f"color: {C_MUTED}; font-size: 11px;")
        self.lbl_power_max_lbl = QLabel("600 W")
        self.lbl_power_max_lbl.setStyleSheet(f"color: {C_MUTED}; font-size: 11px;")
        range_row.addWidget(self.lbl_power_min_lbl)
        range_row.addStretch()
        range_row.addWidget(self.lbl_power_max_lbl)
        ctrl_layout.addLayout(range_row)

        btn_row = QHBoxLayout()
        btn_apply_power = QPushButton("⚡  Limit anwenden")
        btn_apply_power.clicked.connect(self._apply_power_limit)
        btn_reset_power = QPushButton("↺  Zurücksetzen")
        btn_reset_power.clicked.connect(self._reset_power_limit)
        btn_row.addWidget(btn_apply_power)
        btn_row.addWidget(btn_reset_power)
        ctrl_layout.addLayout(btn_row)
        layout.addWidget(ctrl)

        # Hinweis
        hint = QLabel(
            "ℹ  Sudo-Rechte erforderlich. Trage in /etc/sudoers ein:\n"
            "  enricow79 ALL=(ALL) NOPASSWD: /run/current-system/sw/bin/nvidia-smi"
        )
        hint.setStyleSheet(
            f"color: {C_MUTED}; font-size: 11px; "
            f"background: {C_SURFACE}; border: 1px solid {C_BORDER}; "
            f"border-radius: 6px; padding: 10px;"
        )
        hint.setWordWrap(True)
        layout.addWidget(hint)
        layout.addStretch()
        return w

    # ── Lüfter Tab ─────────────────────────────────────────────────────────
    def _build_fan_tab(self) -> QWidget:
        w = QWidget()
        layout = QVBoxLayout(w)
        layout.setContentsMargins(12, 12, 12, 12)
        layout.setSpacing(16)

        # Status
        status = ControlCard("LÜFTERSTATUS")
        status_layout = QGridLayout(status)
        status_layout.setSpacing(10)

        status_layout.addWidget(QLabel("Aktuelle Drehzahl:"), 0, 0)
        self.lbl_fan_cur = QLabel("— %")
        self.lbl_fan_cur.setStyleSheet(f"color: {C_ACCENT2}; font-size: 20px; font-weight: 700;")
        status_layout.addWidget(self.lbl_fan_cur, 0, 1)

        status_layout.addWidget(QLabel("Modus:"), 1, 0)
        self.lbl_fan_mode = QLabel("Automatisch")
        self.lbl_fan_mode.setStyleSheet(f"color: {C_ACCENT}; font-weight: 600;")
        status_layout.addWidget(self.lbl_fan_mode, 1, 1)
        layout.addWidget(status)

        # Manuelle Steuerung
        ctrl = ControlCard("MANUELLE LÜFTERSTEUERUNG  (benötigt sudo)")
        ctrl_layout = QVBoxLayout(ctrl)
        ctrl_layout.setSpacing(10)

        self.lbl_fan_slider_val = QLabel("50 %")
        self.lbl_fan_slider_val.setStyleSheet(
            f"color: {C_ACCENT2}; font-size: 22px; font-weight: 700; qproperty-alignment: AlignCenter;"
        )
        ctrl_layout.addWidget(self.lbl_fan_slider_val)

        self.slider_fan = QSlider(Qt.Orientation.Horizontal)
        self.slider_fan.setRange(0, 100)
        self.slider_fan.setValue(50)
        self.slider_fan.valueChanged.connect(
            lambda v: self.lbl_fan_slider_val.setText(f"{v} %")
        )
        ctrl_layout.addWidget(self.slider_fan)

        # Schieberegler-Stil für Lüfter (Cyan)
        self.slider_fan.setStyleSheet(f"""
            QSlider::sub-page:horizontal {{ background: {C_ACCENT2}; border-radius: 3px; }}
            QSlider::handle:horizontal {{ background: {C_ACCENT2}; }}
        """)

        range_row = QHBoxLayout()
        lbl_0 = QLabel("0 %")
        lbl_0.setStyleSheet(f"color: {C_MUTED}; font-size: 11px;")
        lbl_100 = QLabel("100 %")
        lbl_100.setStyleSheet(f"color: {C_MUTED}; font-size: 11px;")
        range_row.addWidget(lbl_0)
        range_row.addStretch()
        range_row.addWidget(lbl_100)
        ctrl_layout.addLayout(range_row)

        btn_row = QHBoxLayout()
        btn_apply_fan = QPushButton("🌀  Drehzahl setzen")
        btn_apply_fan.clicked.connect(self._apply_fan_speed)
        btn_auto_fan = QPushButton("🔄  Automatisch")
        btn_auto_fan.clicked.connect(self._set_fan_auto)
        btn_row.addWidget(btn_apply_fan)
        btn_row.addWidget(btn_auto_fan)
        ctrl_layout.addLayout(btn_row)
        layout.addWidget(ctrl)

        # Hinweis
        hint = QLabel(
            "ℹ  Sudo-Rechte erforderlich. Für automatische Kontrolle auf 'Automatisch' klicken.\n"
            "    Manuelle Steuerung wird nach Neustart zurückgesetzt."
        )
        hint.setStyleSheet(
            f"color: {C_MUTED}; font-size: 11px; "
            f"background: {C_SURFACE}; border: 1px solid {C_BORDER}; "
            f"border-radius: 6px; padding: 10px;"
        )
        hint.setWordWrap(True)
        layout.addWidget(hint)
        layout.addStretch()
        return w

    # ── Info Tab ───────────────────────────────────────────────────────────
    def _build_info_tab(self) -> QWidget:
        w = QWidget()
        layout = QVBoxLayout(w)
        layout.setContentsMargins(12, 12, 12, 12)
        layout.setSpacing(12)

        info = ControlCard("GPU INFORMATIONEN")
        info_layout = QGridLayout(info)
        info_layout.setSpacing(10)

        self.info_labels = {}
        rows = [
            ("GPU Modell",     "gpu_name"),
            ("Treiber",        "driver"),
            ("CUDA Version",   "cuda"),
            ("VRAM Total",     "vram"),
            ("PCIe Gen",       "pcie_gen"),
            ("PCIe Breite",    "pcie_width"),
            ("BIOS Version",   "bios"),
        ]
        for i, (label, key) in enumerate(rows):
            lbl = QLabel(f"{label}:")
            lbl.setStyleSheet(f"color: {C_MUTED};")
            val = QLabel("—")
            val.setStyleSheet(f"color: {C_TEXT}; font-weight: 600;")
            info_layout.addWidget(lbl, i, 0)
            info_layout.addWidget(val, i, 1)
            self.info_labels[key] = val

        layout.addWidget(info)

        # Sudo-Einrichtung
        sudo_box = ControlCard("SUDO EINRICHTEN (für Power & Fan Control)")
        sudo_layout = QVBoxLayout(sudo_box)
        sudo_hint = QLabel(
            "Um nvidia-smi ohne Passwort-Abfrage nutzen zu können:\n\n"
            "1.  sudo visudo\n\n"
            "2.  Diese Zeile hinzufügen:\n"
            "    enricow79 ALL=(ALL) NOPASSWD: /run/current-system/sw/bin/nvidia-smi\n\n"
            "3.  Speichern und neu anmelden."
        )
        sudo_hint.setStyleSheet(
            f"color: {C_TEXT}; font-size: 12px; font-family: monospace; "
            f"background: {C_BG}; border-radius: 6px; padding: 12px;"
        )
        sudo_hint.setWordWrap(True)
        sudo_layout.addWidget(sudo_hint)
        layout.addWidget(sudo_box)
        layout.addStretch()
        return w

    # ── Tray ───────────────────────────────────────────────────────────────
    def _init_tray(self):
        self.tray = QSystemTrayIcon(make_tray_icon(), self)
        self.tray.setToolTip("NVIDIA GPU Manager")

        menu = QMenu()
        menu.setStyleSheet(f"""
            QMenu {{ background: {C_SURFACE}; color: {C_TEXT}; border: 1px solid {C_BORDER}; }}
            QMenu::item:selected {{ background: {C_CARD}; color: {C_ACCENT}; }}
        """)

        act_show = QAction("🖥  Fenster anzeigen", self)
        act_show.triggered.connect(self.showNormal)
        menu.addAction(act_show)

        menu.addSeparator()

        self.act_tray_stats = QAction("GPU: —", self)
        self.act_tray_stats.setEnabled(False)
        menu.addAction(self.act_tray_stats)

        menu.addSeparator()

        act_quit = QAction("✕  Beenden", self)
        act_quit.triggered.connect(QApplication.quit)
        menu.addAction(act_quit)

        self.tray.setContextMenu(menu)
        self.tray.activated.connect(self._tray_activated)
        self.tray.show()

    def _tray_activated(self, reason):
        if reason == QSystemTrayIcon.ActivationReason.Trigger:
            if self.isVisible():
                self.hide()
            else:
                self.showNormal()
                self.activateWindow()

    # ── Timer & Refresh ────────────────────────────────────────────────────
    def _init_timer(self):
        self.timer = QTimer(self)
        self.timer.timeout.connect(self._refresh)
        self.timer.start(1500)  # alle 1.5 Sekunden

        # GPU-Info einmalig laden
        QTimer.singleShot(200, self._load_gpu_info)

    def _refresh(self):
        stats = get_gpu_stats()
        if not stats:
            self.status_bar.setText("⚠  nvidia-smi nicht erreichbar")
            return

        # Monitoring-Tab
        self.card_temp.update_value(stats["temp"])
        self.card_gpu.update_value(stats["gpu_util"])
        self.card_power.update_value(stats["power_draw"])
        self.card_fan.update_value(stats["fan"])

        self.bar_gpu.setValue(int(stats["gpu_util"]))
        self.lbl_gpu_pct.setText(f"{stats['gpu_util']:.0f} %")

        mem_pct = int(stats["mem_used"] / stats["mem_total"] * 100) if stats["mem_total"] > 0 else 0
        self.bar_mem.setValue(mem_pct)
        self.lbl_mem_val.setText(f"{stats['mem_used']:.0f} / {stats['mem_total']:.0f} MiB")

        self.bar_temp.setValue(int(stats["temp"]))
        self.lbl_temp_val.setText(f"{stats['temp']:.0f} °C")

        self.lbl_clk_gpu.setText(f"{stats['clk_gpu']:.0f} MHz")
        self.lbl_clk_gpu_max.setText(f"{stats['clk_gpu_max']:.0f} MHz")
        self.lbl_clk_mem.setText(f"{stats['clk_mem']:.0f} MHz")
        self.lbl_clk_mem_max.setText(f"{stats['clk_mem_max']:.0f} MHz")

        # Power-Tab
        self.lbl_power_draw.setText(f"{stats['power_draw']:.1f} W")
        self.lbl_power_limit_cur.setText(f"{stats['power_limit']:.0f} W")
        if stats["power_min"] > 0 and stats["power_max"] > 0:
            self.lbl_power_range.setText(
                f"{stats['power_min']:.0f} W  /  {stats['power_max']:.0f} W"
            )
            self.slider_power.setRange(int(stats["power_min"]), int(stats["power_max"]))
            self.lbl_power_min_lbl.setText(f"{stats['power_min']:.0f} W")
            self.lbl_power_max_lbl.setText(f"{stats['power_max']:.0f} W")
            if self.slider_power.value() == 250:
                self.slider_power.setValue(int(stats["power_limit"]))
                self.lbl_power_slider_val.setText(f"{stats['power_limit']:.0f} W")

        # Lüfter-Tab
        self.lbl_fan_cur.setText(f"{stats['fan']:.0f} %")

        # Info-Tab PCIe
        self.info_labels["pcie_gen"].setText(f"PCIe Gen {stats['pcie_gen']}")
        self.info_labels["pcie_width"].setText(f"x{stats['pcie_width']}")

        # Tray aktualisieren
        self.tray.setIcon(make_tray_icon(stats["temp"]))
        self.act_tray_stats.setText(
            f"🌡 {stats['temp']:.0f}°C  ⚡ {stats['power_draw']:.0f}W  "
            f"🖥 {stats['gpu_util']:.0f}%  🌀 {stats['fan']:.0f}%"
        )
        self.tray.setToolTip(
            f"NVIDIA GPU Manager\n"
            f"Temp: {stats['temp']:.0f}°C\n"
            f"GPU: {stats['gpu_util']:.0f}%\n"
            f"Power: {stats['power_draw']:.0f}W"
        )

        # Status
        self.status_bar.setText(
            f"⬤  Live  ·  "
            f"Temp: {stats['temp']:.0f}°C  ·  "
            f"GPU: {stats['gpu_util']:.0f}%  ·  "
            f"VRAM: {mem_pct}%  ·  "
            f"Power: {stats['power_draw']:.0f}/{stats['power_limit']:.0f} W  ·  "
            f"Lüfter: {stats['fan']:.0f}%"
        )

    def _load_gpu_info(self):
        stats = get_gpu_stats()
        if stats:
            self.lbl_gpu_name.setText(stats.get("name", "NVIDIA GPU Manager"))
            self.info_labels["gpu_name"].setText(stats.get("name", "—"))
            self.info_labels["vram"].setText(f"{stats.get('mem_total', 0):.0f} MiB")

        driver = run_smi("--query-gpu=driver_version", "--format=csv,noheader")
        cuda   = run_smi("--query-gpu=cuda_version",   "--format=csv,noheader")
        bios   = run_smi("--query-gpu=vbios_version",  "--format=csv,noheader")

        self.info_labels["driver"].setText(driver or "—")
        self.info_labels["cuda"].setText(cuda or "—")
        self.info_labels["bios"].setText(bios or "—")
        self.lbl_driver.setText(f"Treiber {driver}  ·  CUDA {cuda}")

    # ── Aktionen ───────────────────────────────────────────────────────────
    def _apply_power_limit(self):
        watts = self.slider_power.value()
        self.status_bar.setText(f"⏳  Setze Power Limit auf {watts} W …")
        if set_power_limit(watts):
            self.status_bar.setText(f"✅  Power Limit auf {watts} W gesetzt")
        else:
            self.status_bar.setText("❌  Fehler – sudo-Rechte prüfen (siehe Info-Tab)")

    def _reset_power_limit(self):
        self.status_bar.setText("⏳  Setze Standard-Power-Limit …")
        result = subprocess.run(
            ["sudo", "nvidia-smi", "-i", "0", "--reset-gpu-clocks"],
            capture_output=True
        )
        if result.returncode == 0:
            self.status_bar.setText("✅  Power Limit zurückgesetzt")
        else:
            self.status_bar.setText("❌  Fehler beim Zurücksetzen")

    def _apply_fan_speed(self):
        pct = self.slider_fan.value()
        self.status_bar.setText(f"⏳  Setze Lüfter auf {pct}% …")
        if set_fan_speed(pct):
            self.fan_manual = True
            self.lbl_fan_mode.setText(f"Manuell ({pct}%)")
            self.lbl_fan_mode.setStyleSheet(f"color: {C_WARN}; font-weight: 600;")
            self.status_bar.setText(f"✅  Lüfter auf {pct}% gesetzt")
        else:
            self.status_bar.setText("❌  Fehler – sudo-Rechte prüfen (siehe Info-Tab)")

    def _set_fan_auto(self):
        self.status_bar.setText("⏳  Aktiviere automatische Lüftersteuerung …")
        if set_fan_auto():
            self.fan_manual = False
            self.lbl_fan_mode.setText("Automatisch")
            self.lbl_fan_mode.setStyleSheet(f"color: {C_ACCENT}; font-weight: 600;")
            self.status_bar.setText("✅  Lüfter auf Automatisch gesetzt")
        else:
            self.status_bar.setText("❌  Fehler – sudo-Rechte prüfen (siehe Info-Tab)")

    # ── Fenster schließen → in Tray minimieren ─────────────────────────────
    def closeEvent(self, event):
        event.ignore()
        self.hide()
        self.tray.showMessage(
            "NVIDIA GPU Manager",
            "Läuft im Hintergrund. Tray-Icon anklicken zum Öffnen.",
            QSystemTrayIcon.MessageIcon.Information,
            2000
        )


# ── Einstiegspunkt ────────────────────────────────────────────────────────────
def main():
    signal.signal(signal.SIGINT, signal.SIG_DFL)
    app = QApplication(sys.argv)
    app.setApplicationName("nvidia-manager")
    app.setQuitOnLastWindowClosed(False)

    window = NvidiaManager()
    window.show()

    sys.exit(app.exec())


if __name__ == "__main__":
    main()
