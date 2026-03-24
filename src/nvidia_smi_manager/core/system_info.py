"""System information utilities"""

import psutil
from dataclasses import dataclass
from typing import Optional


@dataclass
class SystemInfo:
    """System information data class"""
    cpu_percent: float
    memory_percent: float
    disk_percent: float
    load_average: tuple  # 1min, 5min, 15min


class SystemMonitor:
    """Monitor system resources"""

    @staticmethod
    def get_system_info() -> SystemInfo:
        """Get current system information"""
        cpu_percent = psutil.cpu_percent(interval=1)
        memory_percent = psutil.virtual_memory().percent
        disk_percent = psutil.disk_usage('/').percent
        load_average = psutil.getloadavg()
        
        return SystemInfo(
            cpu_percent=cpu_percent,
            memory_percent=memory_percent,
            disk_percent=disk_percent,
            load_average=load_average
        )

    @staticmethod
    def get_cpu_count() -> dict:
        """Get CPU core count"""
        return {
            'physical': psutil.cpu_count(logical=False),
            'logical': psutil.cpu_count(logical=True)
        }

    @staticmethod
    def get_memory_info() -> dict:
        """Get memory information"""
        mem = psutil.virtual_memory()
        return {
            'total': mem.total,
            'available': mem.available,
            'percent': mem.percent,
            'used': mem.used,
            'free': mem.free
        }
