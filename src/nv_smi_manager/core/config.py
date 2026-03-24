"""Configuration management"""

from dataclasses import dataclass, field
from typing import Optional, Dict, Any
import json
from pathlib import Path


def _default_gpu_limits() -> Dict[str, tuple]:
    """Default GPU safety limits"""
    return {
        "core_clock_range": (-75, 100),  # MHz
        "memory_clock_range": (-75, 100),  # MHz
        "power_limit_range": (0, 500),  # Watts
    }


@dataclass
class Config:
    """Application configuration"""
    refresh_interval: int = 2  # seconds
    update_frequency: int = 5  # updates between checks
    log_level: str = "INFO"
    enable_alerts: bool = True
    temp_threshold: float = 80.0  # celsius
    power_threshold: float = 90.0  # percentage
    custom_config: dict = field(default_factory=dict)
    # GPU control settings
    gpu_profiles: Dict[str, Dict[str, Any]] = field(default_factory=dict)  # GPU index -> profile
    gpu_limits: Dict[str, tuple] = field(default_factory=_default_gpu_limits)  # Safety limits per setting

    @classmethod
    def from_file(cls, config_path: Path) -> "Config":
        """Load configuration from JSON file"""
        if config_path.exists():
            with open(config_path, 'r') as f:
                data = json.load(f)
            return cls(**data)
        return cls()

    def save_to_file(self, config_path: Path) -> None:
        """Save configuration to JSON file"""
        config_path.parent.mkdir(parents=True, exist_ok=True)
        with open(config_path, 'w') as f:
            json.dump(self.__dict__, f, indent=2)

    def set_gpu_profile(self, gpu_index: int, profile: Dict[str, Any]) -> None:
        """
        Set or update profile for a specific GPU
        
        Args:
            gpu_index: GPU index
            profile: Profile dict with keys: core_clock_offset, memory_clock_offset, power_limit
        """
        self.gpu_profiles[str(gpu_index)] = profile

    def get_gpu_profile(self, gpu_index: int) -> Optional[Dict[str, Any]]:
        """Get profile for a specific GPU"""
        return self.gpu_profiles.get(str(gpu_index))

    def get_all_gpu_profiles(self) -> Dict[str, Dict[str, Any]]:
        """Get all GPU profiles"""
        return self.gpu_profiles

    def set_gpu_limits(self, limits: Dict[str, tuple]) -> None:
        """Set custom safety limits for GPU controls"""
        self.gpu_limits = limits

    @staticmethod
    def get_default_config_path() -> Path:
        """Get default configuration file path"""
        return Path.home() / '.config' / 'nvidia-smi-manager' / 'config.json'
